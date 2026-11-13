use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::*,
    *,
};
use ritkraken_git::NewRemote;

use super::{
    Client, Escape, Message, RepoOperation, Submit,
    modals::{ContentModal, ModalHandle},
    theme::palette,
    widgets::{Tone, button, error_text},
};
use crate::i18n::tr;

struct RemoteForm {
    repo_id: u64,
    client: WeakEntity<Client>,
    inputs: [Entity<InputState>; 3],
    modal: ModalHandle,
    error: Option<Message>,
    _inputs: [Subscription; 3],
}

impl Client {
    pub(super) fn open_add_remote(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab(id).is_none_or(|tab| tab.busy) || self.modal_is_present() {
            return;
        }
        let client = cx.entity().downgrade();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let inputs = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
            let subscriptions = inputs.each_ref().map(|input| {
                cx.subscribe(input, |this: &mut RemoteForm, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.error = None;
                        cx.notify();
                    }
                })
            });
            RemoteForm {
                repo_id: id,
                client,
                inputs,
                modal: handle,
                error: None,
                _inputs: subscriptions,
            }
        });
        let focus = view.read(cx).inputs[0].read(cx).focus_handle(cx);
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.remotes.add"),
                view,
                size(px(500.), px(650.)),
                true,
            )
            .focus(focus),
            window,
            cx,
        );
    }
}

impl RemoteForm {
    fn request(&self, cx: &App) -> ritkraken_git::Result<NewRemote> {
        let [name, pull, push] = self.inputs.each_ref().map(|input| input.read(cx).value());
        NewRemote::new(&name, &pull, Some(&push))
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_busy() || !self.modal.is_open() {
            return;
        }
        let request = match self.request(cx) {
            Ok(request) => request,
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        let Some(client) = self.client.upgrade() else {
            self.error = Some(Message::Key("app.error.repository_unavailable"));
            cx.notify();
            return;
        };
        let form = cx.entity().downgrade();
        let id = self.repo_id;
        self.modal.set_busy(true);
        let modal = self.modal.clone();
        self.error = None;
        let started = client.update(cx, |client, cx| {
            client.update_repo_with_completion(
                id,
                RepoOperation::AddRemote(request),
                move |client, succeeded, window, cx| {
                    if succeeded {
                        client.set_sidebar(id, false, Some(ritkraken_git::RefKind::Remote), cx);
                    }
                    let failure = if succeeded {
                        None
                    } else {
                        Some(
                            client
                                .tab(id)
                                .and_then(|tab| tab.notice.as_ref())
                                .map_or_else(
                                    || tr!("app.error.repository_unavailable"),
                                    |(_, message)| message.text(),
                                ),
                        )
                    };
                    let close = form.update(cx, |this, cx| {
                        if !this.modal.is_open() {
                            return false;
                        }
                        this.modal.set_busy(false);
                        if let Some(failure) = failure {
                            this.error = Some(Message::detail("app.remotes.failed", failure));
                        }
                        cx.notify();
                        window.refresh();
                        succeeded
                    });
                    if matches!(close, Ok(true)) {
                        client.close_content_modal(&modal, window, cx);
                    }
                },
                window,
                cx,
            )
        });
        if let Err(error) = started {
            self.modal.set_busy(false);
            self.error = Some(error);
        }
        cx.notify();
        window.refresh();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal.close(window, cx);
    }
}

impl Render for RemoteForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let busy = self.modal.is_busy();
        div()
            .id("add-remote-form")
            .key_context("CommitOperation")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(14.))
            .text_size(px(12.))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.submit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close(window, cx);
            }))
            .children(
                [
                    "app.remotes.name",
                    "app.remotes.pull_url",
                    "app.remotes.push_url",
                ]
                .into_iter()
                .zip(&self.inputs)
                .map(|(label, input)| {
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(div().text_color(rgb(p.text_muted)).child(tr!(label)))
                        .child(Input::new(input).w_full().text_size(px(12.)).disabled(busy))
                }),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(p.text_muted))
                    .child(tr!("app.remotes.push_hint")),
            )
            .when_some(self.error.as_ref(), |body, error| {
                body.child(error_text(error, cx))
            })
            .child(
                button(
                    "add-remote-submit",
                    tr!(if busy {
                        "app.remotes.adding"
                    } else {
                        "app.remotes.add"
                    }),
                    Tone::Green,
                    !busy && self.request(cx).is_ok(),
                    cx,
                    cx.listener(|this, _, window, cx| this.submit(window, cx)),
                )
                .w_full()
                .h(px(34.))
                .rounded_full(),
            )
    }
}
