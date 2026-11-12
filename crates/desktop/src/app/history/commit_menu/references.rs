use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{CommitMenuState, CommitOperation};

use super::commands::operation_title;
use crate::{
    app::{Client, widgets::name_dialog::NameDialog},
    i18n::tr,
};

impl Client {
    /// Opening a name form needs only the menu snapshot. Read Git on submission.
    pub(super) fn open_reference_dialog(
        &mut self,
        id: u64,
        expected: Arc<CommitMenuState>,
        operation: CommitOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = operation_title(&operation, &expected);
        let (initial, checkbox, hint) = match &operation {
            CommitOperation::CreateBranch {
                name,
                checkout,
                track,
            } => (
                name.as_str(),
                Some((tr!("app.commit_menu.checkout_after_create"), *checkout)),
                track
                    .as_ref()
                    .map(|track| tr!("app.commit_menu.track", branch = track)),
            ),
            CommitOperation::CreateTag(name) => (name.as_str(), None, None),
            _ => unreachable!(),
        };
        let owner = cx.weak_entity();
        let submitted = operation.clone();
        NameDialog::open(
            title,
            initial,
            checkbox,
            hint,
            window,
            cx,
            move |name, checked, window, cx| {
                let mut operation = submitted.clone();
                match &mut operation {
                    CommitOperation::CreateBranch {
                        name: value,
                        checkout,
                        ..
                    } => {
                        *value = name;
                        *checkout = checked;
                    }
                    CommitOperation::CreateTag(value) => *value = name,
                    _ => unreachable!(),
                }
                let _ = owner.update(cx, |client, cx| {
                    client.load_graph_operation(id, expected.clone(), operation, true, window, cx);
                });
            },
        );
    }
}
