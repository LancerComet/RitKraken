use std::sync::Arc;

use gpui_kit::{
    component::{
        checkbox::Checkbox,
        input::{Textarea, TextareaState},
    },
    prelude::*,
    *,
};
use ritkraken_git::{CommitContext, CommitOperation};

use super::commands::operation_title;
use crate::{
    app::{
        Client, Escape, Message, Submit,
        modals::{ContentModal, ModalHandle},
        theme::palette,
        widgets::{Tone, button, error_text},
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
    error: Option<Message>,
    modal: ModalHandle,
}

impl CommitDialog {
    pub(super) fn open(
        id: u64,
        context: Arc<CommitContext>,
        operation: CommitOperation,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let title = operation_title(&operation, &context);
        let owner = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let message = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(6)
                    .default_value(context.message.clone())
            });
            Self {
                id,
                owner,
                context,
                operation,
                message,
                include_staged: false,
                error: None,
                modal: handle,
            }
        });
        let focus = view.read(cx).message.read(cx).focus_handle(cx);
        Client::present_content_modal(
            ContentModal::new(modal, title, view, size(px(540.), px(797.)), true).focus(focus),
            window,
            cx,
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut operation = self.operation.clone();
        let CommitOperation::Amend {
            message,
            include_staged,
        } = &mut operation
        else {
            return;
        };
        *message = self.message.read(cx).value().to_string();
        *include_staged = self.include_staged;
        if message.trim().is_empty() {
            self.error = Some(Message::Key("app.error.commit_summary"));
            cx.notify();
            return;
        }
        let context = self.context.clone();
        let id = self.id;
        self.modal.close(window, cx);
        let _ = self.owner.update(cx, |client, cx| {
            client.prepare_graph_operation(id, context, operation, window, cx)
        });
    }
}

impl Render for CommitDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let mut content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().text_color(rgb(p.text_muted)).child(format!(
                "{} · {}",
                ritkraken_git::short_id(&self.context.commit.id),
                self.context.commit.subject
            )));
        if matches!(self.operation, CommitOperation::Amend { .. }) {
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
                element.child(error_text(error, cx))
            })
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(rgb(p.border))
                    .pt(px(12.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            "cancel-commit-operation",
                            tr!("app.common.cancel"),
                            Tone::Outline,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.modal.close(window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    )
                    .child(
                        button(
                            "apply-commit-operation",
                            tr!("app.commit_menu.apply"),
                            Tone::Green,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.submit(window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    ),
            )
    }
}
