use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::*,
    *,
};
use ritkraken_git::{RecoveryHistory, Repo};

use super::{
    Client, Escape, Load, Message, RepoOperation,
    modals::{ContentModal, ModalHandle},
    theme::palette,
    widgets::{Tone, button, error_text},
};
use crate::i18n::tr;

struct RecoveryForm {
    repo_id: u64,
    repo: Repo,
    client: WeakEntity<Client>,
    modal: ModalHandle,
    history: Load<RecoveryHistory>,
    selected: Option<usize>,
    branch: Entity<InputState>,
    error: Option<Message>,
    _branch: Subscription,
}

impl Client {
    pub(super) fn open_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_is_present() {
            return;
        }
        let Some(tab) = self.current().filter(|tab| !tab.busy) else {
            return;
        };
        let id = tab.id;
        let repo = tab.repo.clone();
        let client = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let branch = cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!("app.recovery.branch_name")));
            let subscription = cx
                .subscribe(&branch, |_: &mut RecoveryForm, _, _: &InputEvent, cx| {
                    cx.notify()
                });
            RecoveryForm {
                repo_id: id,
                repo,
                client,
                modal: handle,
                history: Load::Pending,
                selected: None,
                branch,
                error: None,
                _branch: subscription,
            }
        });
        view.update(cx, |form, cx| form.reload(window, cx));
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.recovery.title"),
                view,
                size(px(860.), px(680.)),
                false,
            ),
            window,
            cx,
        );
    }
}

impl RecoveryForm {
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.repo.clone();
        self.history = Load::Pending;
        self.selected = None;
        Client::spawn_background(
            window,
            cx,
            move || repo.recovery_history(),
            |this, result, _, cx| {
                this.history = result.into();
                cx.notify();
            },
        );
    }

    fn run(&mut self, operation: RepoOperation, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_busy() || !self.modal.is_open() {
            return;
        }
        let id = self.repo_id;
        let form = cx.weak_entity();
        self.error = None;
        self.modal.set_busy(true);
        let result = self.client.update(cx, |client, cx| {
            client.update_repo_with_completion(
                id,
                operation,
                move |client, succeeded, window, cx| {
                    let error = (!succeeded).then(|| {
                        client
                            .tab(id)
                            .and_then(|tab| tab.notice.as_ref())
                            .map_or_else(
                                || tr!("app.error.repository_unavailable"),
                                |notice| notice.message.text(),
                            )
                    });
                    let _ = form.update(cx, |this, cx| {
                        this.modal.set_busy(false);
                        if let Some(error) = error {
                            this.error = Some(Message::detail("app.error.operation", error));
                        } else {
                            this.reload(window, cx);
                        }
                        cx.notify();
                        window.refresh();
                    });
                },
                window,
                cx,
            )
        });
        let failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(Message::detail("app.error.operation", error)),
        };
        if let Some(error) = failure {
            self.modal.set_busy(false);
            self.error = Some(error);
        }
        cx.notify();
        window.refresh();
    }
}

impl Render for RecoveryForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let busy = self.modal.is_busy();
        let history = match &self.history {
            Load::Ready(history) => Some(history),
            _ => None,
        };
        let selected =
            history.and_then(|history| self.selected.and_then(|index| history.entries.get(index)));
        let undo = history.and_then(|history| {
            history
                .undo_target
                .clone()
                .map(|target| (history.clone(), target))
        });
        let restore = history
            .zip(selected)
            .map(|(history, entry)| (history.clone(), entry.id.clone()));
        let branch_name = self.branch.read(cx).value().trim().to_owned();
        div()
            .id("recovery-form")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .child(
                div()
                    .text_color(rgb(p.text_muted))
                    .child(tr!("app.recovery.scope")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        "undo-head",
                        tr!("app.recovery.undo"),
                        Tone::Primary,
                        !busy && undo.is_some(),
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            if let Some((history, target)) = &undo {
                                this.run(
                                    RepoOperation::RestoreHead(history.clone(), target.clone()),
                                    window,
                                    cx,
                                );
                            }
                        }),
                    ))
                    .child(button(
                        "refresh-recovery",
                        tr!("app.conflicts.reload"),
                        Tone::Outline,
                        !busy,
                        cx,
                        cx.listener(|this, _, window, cx| this.reload(window, cx)),
                    )),
            )
            .when_some(self.error.as_ref(), |body, error| {
                body.child(error_text(error, cx))
            })
            .child(match &self.history {
                Load::Pending => div().child(tr!("app.recovery.loading")).into_any_element(),
                Load::Failed(error) => error_text(error, cx).into_any_element(),
                Load::Ready(history) if history.entries.is_empty() => {
                    div().child(tr!("app.recovery.empty")).into_any_element()
                }
                Load::Ready(history) => uniform_list(
                    "recovery-entries",
                    history.entries.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let Load::Ready(history) = &this.history else {
                            return Vec::new();
                        };
                        let p = palette(cx);
                        range
                            .map(|index| {
                                let entry = &history.entries[index];
                                div()
                                    .id(("recovery-entry", index))
                                    .w_full()
                                    .h(px(56.))
                                    .px(px(8.))
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .border_b_1()
                                    .border_color(rgb(p.border))
                                    .when(this.selected == Some(index), |row| {
                                        row.bg(rgb(p.accent_soft))
                                    })
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(p.hover)))
                                    .child(div().truncate().child(if entry.message.is_empty() {
                                        tr!("app.recovery.reference_update")
                                    } else {
                                        entry.message.clone()
                                    }))
                                    .child(div().truncate().text_color(rgb(p.text_muted)).child(
                                        format!(
                                            "{}  ·  {}  ·  {}",
                                            ritkraken_git::short_id(&entry.id),
                                            entry.selector,
                                            entry.date
                                        ),
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !this.modal.is_busy() {
                                            this.selected = Some(index);
                                            cx.notify();
                                        }
                                    }))
                            })
                            .collect()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0()
                .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        "restore-head",
                        tr!("app.recovery.restore"),
                        Tone::Outline,
                        !busy && restore.is_some(),
                        cx,
                        cx.listener({
                            let restore = restore.clone();
                            move |this, _, window, cx| {
                                if let Some((history, target)) = &restore {
                                    this.run(
                                        RepoOperation::RestoreHead(history.clone(), target.clone()),
                                        window,
                                        cx,
                                    );
                                }
                            }
                        }),
                    ))
                    .child(Input::new(&self.branch).disabled(busy).flex_1().min_w_0())
                    .child(button(
                        "recover-branch",
                        tr!("app.recovery.create_branch"),
                        Tone::Green,
                        !busy && restore.is_some() && !branch_name.is_empty(),
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            if let Some((history, target)) = &restore {
                                this.run(
                                    RepoOperation::RecoverBranch(
                                        history.clone(),
                                        target.clone(),
                                        branch_name.clone(),
                                    ),
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )),
            )
    }
}
