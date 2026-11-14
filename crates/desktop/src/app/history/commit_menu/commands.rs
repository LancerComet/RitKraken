use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{self as git, CommitContext, CommitMenuState, CommitOperation, ResetMode};

use super::{GraphAction, compare::CommitComparison, dialog::CommitDialog, reset_label};
use crate::{
    app::{Client, Message, Notice, RepoOperation},
    i18n::tr,
};

impl Client {
    pub(in crate::app::history::commit_menu) fn run_graph_action(
        &mut self,
        id: u64,
        target: &str,
        context: Option<Arc<CommitMenuState>>,
        action: GraphAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(action, GraphAction::CopyHash) {
            cx.write_to_clipboard(ClipboardItem::new_string(target.to_owned()));
            return;
        }
        let Some(context) = context else { return };
        let Some(tab) = self.tab(id) else { return };
        match action {
            GraphAction::CopyHash => unreachable!(),
            GraphAction::CopyMessage => {
                if tab.history.selected.as_deref() == Some(context.commit.id.as_str())
                    && let crate::app::Load::Ready(detail) = &tab.changes.detail
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(detail.message.clone()));
                } else {
                    let repo = tab.repo.clone();
                    let commit = context.commit.clone();
                    Self::spawn_background(
                        window,
                        cx,
                        move || repo.detail(&commit),
                        move |this, result, window, cx| match result {
                            Ok(detail) => {
                                cx.write_to_clipboard(ClipboardItem::new_string(detail.message))
                            }
                            Err(error) => {
                                if let Some(tab) = this.tab_mut(id) {
                                    tab.notify(Notice::failure(error), window, cx);
                                    cx.notify();
                                }
                            }
                        },
                    );
                }
            }
            GraphAction::OpenRemote(url) => cx.open_url(&url),
            GraphAction::Compare => CommitComparison::open(
                tab.repo.clone(),
                &context,
                &self.saved.preferences,
                window,
                cx,
            ),
            GraphAction::Export(mainline) => {
                let repo = tab.repo.clone();
                let revision = context.commit.id.clone();
                self.save_patch(
                    id,
                    format!("{}.patch", git::short_id(&revision)),
                    move |path| {
                        repo.export_commit_patch(&revision, mainline, &path)
                            .map_err(Message::from)
                    },
                    window,
                    cx,
                );
            }
            GraphAction::Operation(operation) if !tab.busy && self.session.active == Some(id) => {
                if matches!(
                    operation,
                    CommitOperation::CreateBranch { .. } | CommitOperation::CreateTag(_)
                ) {
                    self.open_reference_dialog(id, context, operation, window, cx);
                } else {
                    self.load_graph_operation(id, context, operation, false, window, cx);
                }
            }
            GraphAction::Operation(_) => {}
        }
    }

    pub(in crate::app) fn load_graph_operation(
        &mut self,
        id: u64,
        expected: Arc<CommitMenuState>,
        mut operation: CommitOperation,
        reviewed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        let repo = tab.repo.clone();
        let target = expected.commit.id.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                let context = repo.commit_context(&target)?;
                if !context.same_checkout(&expected) {
                    return Err(git::RuleError::StaleCommit.into());
                }
                context.ensure_available(operation.action())?;
                if let CommitOperation::Amend { message, .. } = &mut operation {
                    *message = context.message.clone();
                } else if let CommitOperation::InteractiveRebase(plan) = &mut operation {
                    *plan = repo.rebase_plan(&context)?;
                }
                Ok((Arc::new(context), operation))
            },
            move |this, result: git::Result<_>, window, cx| {
                let active = this.session.active == Some(id);
                let Some(tab) = this.tab_mut(id) else { return };
                tab.busy = false;
                if !active {
                    cx.notify();
                    return;
                }
                match result {
                    Ok((context, CommitOperation::InteractiveRebase(plan))) if !reviewed => {
                        this.start_rebase_session(id, context, plan, window, cx)
                    }
                    Ok((context, operation))
                        if !reviewed && matches!(operation, CommitOperation::Amend { .. }) =>
                    {
                        CommitDialog::open(id, context, operation, window, cx)
                    }
                    Ok((context, operation)) => {
                        this.prepare_graph_operation(id, context, operation, reviewed, window, cx)
                    }
                    Err(error) => tab.notify(Notice::failure(error), window, cx),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(in crate::app::history) fn prepare_graph_operation(
        &mut self,
        id: u64,
        context: Arc<CommitContext>,
        operation: CommitOperation,
        reviewed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || repo.prepare_commit_operation(&context, operation),
            move |this, result, window, cx| {
                let Some(tab) = this.tab_mut(id) else { return };
                let prepared = match result {
                    Ok(value) => Arc::new(value),
                    Err(error) => {
                        tab.busy = false;
                        let message = Message::from(error);
                        tab.notify(Notice::failure(message), window, cx);
                        cx.notify();
                        return;
                    }
                };
                let immediate = (reviewed
                    || matches!(
                        prepared.operation,
                        CommitOperation::Switch(_) | CommitOperation::Continue
                    ))
                    && prepared.published.is_empty();
                if immediate {
                    tab.busy = false;
                    this.update_repo(id, Some(RepoOperation::CommitGraph(prepared)), window, cx);
                    return;
                }
                let title = operation_title(&prepared.operation, &prepared.context);
                let branch = prepared.context.branch.as_deref().unwrap_or("HEAD");
                let mut detail = tr!(
                    "app.commit_menu.confirm_detail",
                    repository = tab.repo.name(),
                    branch = branch,
                    head = git::short_id(&prepared.context.head),
                    target = git::short_id(&prepared.context.commit.id),
                    subject = &prepared.context.commit.subject
                );
                if prepared.affected_count > 0 {
                    detail.push_str(&format!(
                        "\n\n{}",
                        tr!("app.commit_menu.affected", count = prepared.affected_count)
                    ));
                }
                if !prepared.published.is_empty() {
                    detail.push_str(&format!(
                        "\n\n{}",
                        tr!(
                            "app.commit_menu.published",
                            refs = prepared.published.join(", ")
                        )
                    ));
                }
                if matches!(prepared.operation, CommitOperation::Reset(ResetMode::Hard)) {
                    detail.push_str(&format!("\n\n{}", tr!("app.commit_menu.hard_detail")));
                }
                // Transfer the preparation reservation to the shared confirmation in this UI turn.
                tab.busy = false;
                this.confirm_repo_operation(
                    id,
                    RepoOperation::CommitGraph(prepared),
                    &title,
                    &detail,
                    window,
                    cx,
                );
            },
        );
        cx.notify();
    }
}

pub(super) fn operation_title(operation: &CommitOperation, context: &CommitMenuState) -> String {
    let branch = context.branch.as_deref().unwrap_or("HEAD");
    match operation {
        CommitOperation::CreateBranch { .. } => tr!("app.commit_menu.create_branch"),
        CommitOperation::CreateTag(_) => tr!("app.commit_menu.create_tag"),
        CommitOperation::Switch(_) => tr!("app.commit_menu.switch_branch"),
        CommitOperation::Checkout => tr!("app.commit_menu.checkout"),
        CommitOperation::CherryPick { .. } => tr!("app.commit_menu.cherry_pick", branch = branch),
        CommitOperation::Revert { .. } => tr!("app.commit_menu.revert"),
        CommitOperation::Merge => tr!("app.commit_menu.merge", branch = branch),
        CommitOperation::Rebase => tr!("app.commit_menu.rebase", branch = branch),
        CommitOperation::InteractiveRebase(_) => tr!("app.commit_menu.interactive"),
        CommitOperation::Amend { .. } => tr!("app.commit_menu.amend"),
        CommitOperation::Reset(mode) => reset_label(*mode),
        CommitOperation::Continue => tr!("app.commit_menu.continue"),
        CommitOperation::Skip => tr!("app.commit_menu.skip"),
        CommitOperation::Abort => tr!("app.commit_menu.abort"),
    }
}
