use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{CommitMenuState, CommitOperation};

use super::commands::operation_title;
use crate::app::{Client, widgets::name_dialog::NameDialog};

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
        if matches!(operation, CommitOperation::CreateBranch { .. }) {
            self.start_inline_branch(id, expected, operation, window, cx);
            return;
        }
        let title = operation_title(&operation, &expected);
        let CommitOperation::CreateTag(initial) = &operation else {
            return;
        };
        let owner = cx.weak_entity();
        NameDialog::open(
            title,
            initial,
            None,
            None,
            window,
            cx,
            move |name, _, window, cx| {
                let operation = CommitOperation::CreateTag(name);
                let _ = owner.update(cx, |client, cx| {
                    client.load_graph_operation(id, expected.clone(), operation, true, window, cx);
                });
            },
        );
    }
}
