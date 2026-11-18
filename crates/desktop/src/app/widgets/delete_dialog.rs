use gpui_kit::{prelude::*, *};

use super::{Tone, button};
use crate::{
    app::{
        Client, Escape, RepoOperation, Submit,
        modals::{ContentModal, ModalHandle},
    },
    i18n::tr,
};

/// Asks before running a destructive repository operation, such as deleting
/// a branch or a stash. Cancel has the initial focus.
pub(in crate::app) struct DeleteDialog {
    repo_id: u64,
    owner: WeakEntity<Client>,
    operation: Option<RepoOperation>,
    message: String,
    modal: ModalHandle,
    cancel_focus: FocusHandle,
}

impl DeleteDialog {
    pub(in crate::app) fn open(
        repo_id: u64,
        title: String,
        message: String,
        operation: RepoOperation,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let owner = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| Self {
            repo_id,
            owner,
            operation: Some(operation),
            message,
            modal: handle,
            cancel_focus: cx.focus_handle(),
        });
        let focus = view.read(cx).cancel_focus.clone();
        Client::present_content_modal(
            ContentModal::new(modal, title, view, size(px(500.), px(650.)), true).focus(focus),
            window,
            cx,
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.modal.is_open() {
            return;
        }
        let Some(operation) = self.operation.take() else {
            return;
        };
        self.modal.close(window, cx);
        let _ = self.owner.update(cx, |client, cx| {
            client.update_repo(self.repo_id, Some(operation), window, cx);
        });
    }
}

impl Render for DeleteDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("delete-dialog")
            .key_context("CommitOperation")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.submit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .child(div().child(self.message.clone()))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            "cancel-delete",
                            tr!("app.common.cancel"),
                            Tone::Outline,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.modal.close(window, cx)),
                        )
                        .track_focus(&self.cancel_focus)
                        .rounded_full()
                        .px(px(14.)),
                    )
                    .child(
                        button(
                            "confirm-delete",
                            tr!("app.ref_menu.confirm_delete"),
                            Tone::Red,
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
