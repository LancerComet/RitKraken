use std::sync::Arc;

use gpui_kit::{
    component::{
        checkbox::Checkbox,
        input::{InputEvent, Textarea, TextareaState},
    },
    prelude::*,
    *,
};
use ritkraken_git::{CommitContext, CommitOperation, RebasePlan, Repo};

use super::commands::operation_title;
use crate::{
    app::{
        Client, Escape, Load, Message, Submit,
        modals::{ContentModal, ModalHandle},
        widgets::{LINE, MUTED, Tone, button, error_text},
    },
    i18n::tr,
};

pub(super) struct CommitDialog {
    pub(super) id: u64,
    pub(super) owner: WeakEntity<Client>,
    pub(super) context: Arc<CommitContext>,
    pub(super) operation: CommitOperation,
    pub(super) message: Entity<TextareaState>,
    pub(super) include_staged: bool,
    pub(super) plan: Option<Load<RebasePlan>>,
    pub(super) selected: usize,
    pub(super) scroll: UniformListScrollHandle,
    error: Option<Message>,
    modal: ModalHandle,
    _message: Subscription,
}

impl CommitDialog {
    pub(super) fn open(
        id: u64,
        repo: Repo,
        context: Arc<CommitContext>,
        operation: CommitOperation,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let title = operation_title(&operation, &context);
        let interactive = matches!(operation, CommitOperation::InteractiveRebase(_));
        let owner = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let message = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(6)
                    .default_value(context.message.clone())
            });
            let subscription = cx.subscribe_in(
                &message,
                window,
                |this: &mut Self, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.sync_plan_message(cx);
                        cx.notify();
                    }
                },
            );
            let state = Self {
                id,
                owner,
                context,
                operation,
                message,
                include_staged: false,
                plan: interactive.then_some(Load::Pending),
                selected: 0,
                scroll: UniformListScrollHandle::default(),
                error: None,
                modal: handle,
                _message: subscription,
            };
            if interactive {
                let context = state.context.clone();
                Client::spawn_background(
                    window,
                    cx,
                    move || repo.rebase_plan(&context),
                    |this, result, window, cx| {
                        this.plan = Some(result.into());
                        this.select_plan_entry(0, window, cx);
                        cx.notify();
                    },
                );
            }
            state
        });
        let focus = view.read(cx).message.read(cx).focus_handle(cx);
        Client::present_content_modal(
            ContentModal::new(
                modal,
                title,
                view,
                size(px(if interactive { 1100. } else { 540. }), px(797.)),
                !interactive,
            )
            .focus(focus),
            window,
            cx,
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_plan_message(cx);
        let mut operation = self.operation.clone();
        match &mut operation {
            CommitOperation::Amend {
                message,
                include_staged,
            } => {
                *message = self.message.read(cx).value().to_string();
                *include_staged = self.include_staged;
                if message.trim().is_empty() {
                    self.error = Some(Message::Key("app.error.commit_summary"));
                    cx.notify();
                    return;
                }
            }
            CommitOperation::InteractiveRebase(plan) => {
                let Some(Load::Ready(edited)) = &self.plan else {
                    return;
                };
                *plan = edited.clone();
            }
            _ => return,
        }
        let context = self.context.clone();
        let id = self.id;
        self.modal.close(window, cx);
        let _ = self.owner.update(cx, |client, cx| {
            client.prepare_graph_operation(id, context, operation, true, window, cx)
        });
    }
}

impl Render for CommitDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let interactive = self.plan.is_some();
        let ready = !matches!(self.plan, Some(Load::Pending | Load::Failed(_)));
        let mut content = div().w_full().min_w_0().flex().flex_col().gap(px(12.));
        if interactive {
            content = content.h_full().child(self.planner_view(cx));
        } else {
            content = content.child(div().text_color(rgb(MUTED)).child(format!(
                "{} · {}",
                ritkraken_git::short_id(&self.context.commit.id),
                self.context.commit.subject
            )));
            match &self.operation {
                CommitOperation::Amend { .. } => {
                    content = content
                        .child(Textarea::new(&self.message).text_size(px(12.)))
                        .child(
                            Checkbox::new("include-staged")
                                .checked(self.include_staged)
                                .label(tr!(
                                    "app.commit_menu.include_staged",
                                    count = self.context.staged_count
                                ))
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    this.include_staged = *checked;
                                    cx.notify();
                                })),
                        );
                }
                _ => {}
            }
        }
        content
            .id("commit-operation-dialog")
            .key_context("CommitOperation")
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.submit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .text_size(px(12.))
            .when_some(self.error.as_ref(), |element, error| {
                element.child(error_text(error))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .pt(px(12.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(button(
                        "cancel-commit-operation",
                        tr!("app.common.cancel"),
                        Tone::Outline,
                        true,
                        cx.listener(|this, _, window, cx| this.modal.close(window, cx)),
                    ))
                    .child(button(
                        "apply-commit-operation",
                        tr!(if interactive {
                            "app.commit_menu.start_rebase"
                        } else {
                            "app.commit_menu.apply"
                        }),
                        Tone::Green,
                        ready,
                        cx.listener(|this, _, window, cx| this.submit(window, cx)),
                    )),
            )
    }
}
