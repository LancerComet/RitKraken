use gpui_kit::*;
use ritkraken_git::{RefKind, RefOperation, RefRequest};

use super::delete::DeleteBranchDialog;

use crate::{
    app::{Client, RepoOperation, widgets::name_dialog::NameDialog},
    i18n::tr,
};

impl Client {
    pub(in crate::app::refs) fn run_ref_action(
        &mut self,
        id: u64,
        request: RefRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_is_present()
            || self.session.active != Some(id)
            || !self.tab(id).is_some_and(|tab| {
                !tab.busy && tab.rebase.is_none() && tab.history.data.snapshot.sequence.is_none()
            })
        {
            return;
        }
        match &request.operation {
            RefOperation::Rename(name) | RefOperation::Track(name) => {
                let title = tr!(if matches!(request.operation, RefOperation::Rename(_)) {
                    "app.ref_menu.rename"
                } else {
                    "app.ref_menu.track"
                });
                let owner = cx.weak_entity();
                let initial = name.clone();
                let hint = request.reference.full_name();
                NameDialog::open(
                    title,
                    &initial,
                    None,
                    Some(hint),
                    window,
                    cx,
                    move |name, _, window, cx| {
                        let mut request = request.clone();
                        match &mut request.operation {
                            RefOperation::Rename(value) | RefOperation::Track(value) => {
                                *value = name
                            }
                            _ => unreachable!(),
                        }
                        let _ = owner.update(cx, |client, cx| {
                            client.update_repo(
                                id,
                                Some(RepoOperation::Reference(request)),
                                window,
                                cx,
                            )
                        });
                    },
                );
            }
            RefOperation::Delete if request.reference.kind != RefKind::Tag => {
                DeleteBranchDialog::open(id, request, window, cx);
            }
            RefOperation::Checkout | RefOperation::Delete | RefOperation::PushTag(_) => {
                self.update_repo(id, Some(RepoOperation::Reference(request)), window, cx)
            }
        }
    }
}
