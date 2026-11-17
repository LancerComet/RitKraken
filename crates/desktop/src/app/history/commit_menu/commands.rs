use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{self as git, CommitContext, CommitMenuState, CommitOperation};

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
                let review = !reviewed
                    && matches!(
                        operation,
                        CommitOperation::InteractiveRebase(_) | CommitOperation::Amend { .. }
                    );
                Ok(if review {
                    Loaded::Review(Arc::new(context), operation)
                } else {
                    // The state was read just now; preparing here avoids a
                    // second full read before the pre-run check.
                    Loaded::Prepared(Box::new(
                        repo.prepare_fresh_commit_operation(context, operation),
                    ))
                })
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
                    Ok(Loaded::Review(context, CommitOperation::InteractiveRebase(plan))) => {
                        this.start_rebase_session(id, context, plan, window, cx)
                    }
                    Ok(Loaded::Review(context, operation)) => {
                        CommitDialog::open(id, context, operation, window, cx)
                    }
                    Ok(Loaded::Prepared(prepared)) => {
                        this.run_prepared_operation(id, *prepared, window, cx)
                    }
                    Err(error) => tab.notify(Notice::failure(error), window, cx),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(in crate::app) fn prepare_graph_operation(
        &mut self,
        id: u64,
        context: Arc<CommitContext>,
        operation: CommitOperation,
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
                tab.busy = false;
                this.run_prepared_operation(id, result, window, cx);
            },
        );
        cx.notify();
    }

    fn run_prepared_operation(
        &mut self,
        id: u64,
        result: git::Result<git::PreparedCommitOperation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prepared = match result {
            Ok(value) => Arc::new(value),
            Err(error) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.notify(Notice::failure(Message::from(error)), window, cx);
                }
                cx.notify();
                return;
            }
        };
        let id = self.finish_rebase_preparation(id, window, cx);
        if matches!(prepared.operation, CommitOperation::Amend { .. }) {
            if let Err(message) = self.update_repo_with_completion(
                id,
                RepoOperation::CommitGraph(prepared),
                move |this, succeeded, window, cx| {
                    if succeeded {
                        this.restore_commit_draft(id, window, cx);
                    }
                },
                window,
                cx,
            ) {
                if let Some(tab) = self.tab_mut(id) {
                    tab.notify(Notice::failure(message), window, cx);
                }
                cx.notify();
            }
        } else {
            self.update_repo(id, Some(RepoOperation::CommitGraph(prepared)), window, cx);
        }
    }
}

/// What a graph operation needs once its repository state has been read.
enum Loaded {
    /// The user reviews the operation in a dialog or rebase session first.
    Review(Arc<CommitContext>, CommitOperation),
    /// The operation is ready to run after its pre-run check.
    Prepared(Box<git::Result<git::PreparedCommitOperation>>),
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
