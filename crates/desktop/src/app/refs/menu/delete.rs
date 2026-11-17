use gpui_kit::{
    component::{
        Sizable,
        button::{Button, ButtonVariants},
    },
    prelude::*,
    *,
};
use ritkraken_git::{RefKind, RefRequest};

use crate::{
    app::{
        Client, Escape, RepoOperation, Submit,
        modals::{ContentModal, ModalHandle},
    },
    i18n::tr,
};

pub(super) struct DeleteBranchDialog {
    repo_id: u64,
    owner: WeakEntity<Client>,
    request: Option<RefRequest>,
    message: String,
    modal: ModalHandle,
    cancel_focus: FocusHandle,
}

impl DeleteBranchDialog {
    pub(super) fn open(
        repo_id: u64,
        request: RefRequest,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let message = tr!(
            if request.reference.kind == RefKind::Remote {
                "app.ref_menu.confirm_delete_remote"
            } else {
                "app.ref_menu.confirm_delete_local"
            },
            name = &request.reference.name
        );
        let owner = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| Self {
            repo_id,
            owner,
            request: Some(request),
            message,
            modal: handle,
            cancel_focus: cx.focus_handle(),
        });
        let focus = view.read(cx).cancel_focus.clone();
        Client::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.ref_menu.confirm_delete_title"),
                view,
                size(px(500.), px(650.)),
                true,
            )
            .focus(focus),
            window,
            cx,
        );
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.modal.is_open() {
            return;
        }
        let Some(request) = self.request.take() else {
            return;
        };
        self.modal.close(window, cx);
        let _ = self.owner.update(cx, |client, cx| {
            client.update_repo(
                self.repo_id,
                Some(RepoOperation::Reference(request)),
                window,
                cx,
            );
        });
    }
}

impl Render for DeleteBranchDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("delete-branch-dialog")
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
                        Button::new("cancel-delete-branch")
                            .label(tr!("app.common.cancel"))
                            .outline()
                            .small()
                            .track_focus(&self.cancel_focus)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.modal.close(window, cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-delete-branch")
                            .label(tr!("app.ref_menu.confirm_delete"))
                            .danger()
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
