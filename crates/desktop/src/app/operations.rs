mod auto_fetch;
mod updates;

pub(super) use updates::RepoUpdates;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::*;
use ritkraken_git::{self as git, Change, FileTarget, Patch, PatchSelection, Repo, Scope};

use super::{
    Client, Message, Notice,
    changes::StageAction,
    history::{History, HistoryState},
    session::ActiveOp,
};

type Completion = Box<dyn FnOnce(&mut Client, bool, &mut Window, &mut Context<Client>)>;

#[derive(Clone)]
pub(super) enum RepoOperation {
    AddWorktree(git::NewWorktree),
    RemoveWorktree(git::Worktree),
    RestoreHead(git::RecoveryHistory, String),
    RecoverBranch(git::RecoveryHistory, String, String),
    Reference(git::RefRequest),
    CommitGraph(Arc<git::PreparedCommitOperation>),
    StageFiles(StageAction, Option<Vec<Change>>),
    StagePatch(FileTarget, Arc<Patch>, PatchSelection),
    ResolveConflict(Arc<git::ConflictFile>, git::ConflictResolution),
    DiscardFiles(Arc<Vec<git::DiscardFile>>),
    IgnoreFile(Change, git::IgnoreRule, bool),
    StashFile(Change),
    Commit(String, String),
    Fetch(Option<String>),
    AddRemote(git::NewRemote),
    UpdateRemote(String, git::NewRemote),
    RemoveRemote(String),
    AutoFetch(Duration),
    Push {
        branch: String,
        upstream: git::Upstream,
        set_upstream: bool,
    },
    ForcePush(git::PushRejection),
    PullRejected(git::PushRejection),
    Pull {
        branch: String,
        upstream: git::Upstream,
        mode: git::PullMode,
    },
    Stash,
    Pop,
    ApplyStash(git::Stash),
    DropStash(git::Stash),
}

impl RepoOperation {
    fn resets_selection(&self) -> bool {
        match self {
            Self::RestoreHead(..) => true,
            Self::Reference(request) => matches!(
                request.operation,
                git::RefOperation::Checkout | git::RefOperation::Track(_)
            ),
            Self::CommitGraph(operation) => !matches!(
                operation.operation,
                git::CommitOperation::CreateTag(_)
                    | git::CommitOperation::CreateBranch {
                        checkout: false,
                        ..
                    }
            ),
            Self::Stash
            | Self::Pop
            | Self::ApplyStash(_)
            | Self::DropStash(_)
            | Self::PullRejected(_)
            | Self::Pull { .. } => true,
            _ => false,
        }
    }

    fn execute(&self, repo: &Repo) -> git::Result<()> {
        match self {
            Self::AddWorktree(request) => repo.add_worktree(request),
            Self::RemoveWorktree(request) => repo.remove_worktree(request),
            Self::RestoreHead(history, target) => repo.restore_head(history, target),
            Self::RecoverBranch(history, target, name) => {
                repo.recover_branch(history, target, name)
            }
            Self::Reference(request) => repo.apply_ref_operation(request),
            Self::CommitGraph(operation) => repo.apply_commit_operation(
                operation,
                &git::GitEditor::for_executable(&std::env::current_exe()?),
            ),
            Self::StageFiles(StageAction::Stage, changes) => repo.stage_files(changes.as_deref()),
            Self::StageFiles(StageAction::Unstage, changes) => {
                repo.unstage_files(changes.as_deref())
            }
            Self::StagePatch(file, patch, selection) => repo.apply_partial(file, patch, *selection),
            Self::ResolveConflict(file, resolution) => repo.resolve_conflict(file, resolution),
            Self::DiscardFiles(expected) => repo.discard_files(expected),
            Self::IgnoreFile(change, rule, stop_tracking) => {
                repo.ignore_file(change, *rule)?;
                if *stop_tracking {
                    repo.stop_tracking(change)?;
                }
                Ok(())
            }
            Self::StashFile(change) => repo.stash_file(change),
            Self::Commit(summary, description) => repo.commit(summary, description),
            Self::Fetch(remote) => repo.fetch(remote.as_deref()),
            Self::AddRemote(remote) => repo.add_remote(remote),
            Self::UpdateRemote(name, remote) => repo.update_remote(name, remote),
            Self::RemoveRemote(name) => repo.remove_remote(name),
            Self::AutoFetch(timeout) => repo.auto_fetch_with_timeout(*timeout),
            Self::Push {
                branch,
                upstream,
                set_upstream,
            } => repo.push(branch, upstream, *set_upstream),
            Self::ForcePush(request) => repo.force_push(request),
            Self::PullRejected(request) => repo.pull_after_rejected_push(request),
            Self::Pull {
                branch,
                upstream,
                mode,
            } => repo.pull(branch, upstream, *mode),
            Self::Stash => repo.stash(),
            Self::Pop => repo.pop_stash(),
            Self::ApplyStash(stash) => repo.apply_stash(stash, false),
            Self::DropStash(stash) => repo.drop_stash(stash),
        }
    }

    fn status_key(&self) -> Option<&'static str> {
        Some(match self {
            Self::AddWorktree(_) | Self::RemoveWorktree(_) => "app.worktrees.completed",
            Self::RestoreHead(..) | Self::RecoverBranch(..) => "app.recovery.completed",
            Self::StageFiles(..) | Self::StagePatch(..) => return None,
            Self::Reference(request)
                if matches!(request.operation, git::RefOperation::PushTag(_)) =>
            {
                "app.notice.tag_pushed"
            }
            Self::Reference(_) => "app.ref_menu.completed",
            Self::CommitGraph(operation)
                if matches!(operation.operation, git::CommitOperation::CherryPick { .. }) =>
            {
                "app.notice.cherry_picked"
            }
            Self::CommitGraph(_) => "app.commit_menu.completed",
            Self::ResolveConflict(..) => "app.conflicts.resolved",
            Self::DiscardFiles(_) => "app.file_menu.discarded",
            Self::IgnoreFile(_, _, false) => "app.file_menu.ignored",
            Self::IgnoreFile(_, _, true) => "app.file_menu.stopped_tracking",
            Self::StashFile(_) => "app.notice.stashed",
            Self::Commit(..) => "app.notice.committed",
            Self::Fetch(_) | Self::AutoFetch(_) => "app.notice.fetched",
            Self::AddRemote(_) => "app.notice.remote_added",
            Self::UpdateRemote(..) => "app.notice.remote_updated",
            Self::RemoveRemote(_) => "app.notice.remote_removed",
            Self::Push { .. } | Self::ForcePush(_) => "app.notice.pushed",
            Self::PullRejected(_) | Self::Pull { .. } => "app.notice.pulled",
            Self::Stash => "app.notice.stashed",
            Self::Pop => "app.notice.popped",
            Self::ApplyStash(_) => "app.notice.stash_applied",
            Self::DropStash(_) => "app.notice.stash_dropped",
        })
    }

    fn activity(&self) -> ActiveOp {
        match self {
            Self::Fetch(_) | Self::AutoFetch(_) | Self::PullRejected(_) | Self::Pull { .. } => {
                ActiveOp::Fetch
            }
            Self::Push { .. } | Self::ForcePush(_) => ActiveOp::Push,
            Self::Reference(request)
                if matches!(request.operation, git::RefOperation::PushTag(_)) =>
            {
                ActiveOp::Push
            }
            Self::Commit(..) => ActiveOp::Commit,
            Self::Stash => ActiveOp::Stash,
            Self::Pop | Self::ApplyStash(_) => ActiveOp::Pop,
            Self::CommitGraph(prepared)
                if matches!(
                    prepared.operation,
                    git::CommitOperation::CreateBranch { .. }
                ) =>
            {
                ActiveOp::Branch
            }
            _ => ActiveOp::Other,
        }
    }
}

impl Client {
    pub(super) fn reveal_commit(
        &mut self,
        id: u64,
        commit: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
            return;
        };
        if tab.history.data.position.contains_key(&commit) {
            self.select(id, Some(commit), Some(ScrollStrategy::Center), window, cx);
            return;
        }
        self.start_repo_update(
            id,
            None,
            None,
            true,
            Some(Box::new(move |this, _, window, cx| {
                this.select(id, Some(commit), Some(ScrollStrategy::Center), window, cx);
            })),
            false,
            window,
            cx,
        );
    }

    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.session.active {
            self.update_repo(id, None, window, cx);
        }
    }

    pub(super) fn update_repo(
        &mut self,
        id: u64,
        edit: Option<RepoOperation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tab(id) {
            self.update_repo_with_limit(id, edit, tab.history.limit, window, cx);
        }
    }

    pub(super) fn update_repo_with_limit(
        &mut self,
        id: u64,
        edit: Option<RepoOperation>,
        limit: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_repo_update(id, edit, limit, true, None, false, window, cx);
    }

    /// A display-only history expansion: reuses the previous snapshot's
    /// status and references, reading only the grown commit list.
    pub(super) fn page_repo_history(
        &mut self,
        id: u64,
        limit: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_repo_update(id, None, Some(limit), true, None, true, window, cx);
    }

    pub(super) fn update_repo_with_completion(
        &mut self,
        id: u64,
        edit: RepoOperation,
        complete: impl FnOnce(&mut Self, bool, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::result::Result<(), Message> {
        let Some(tab) = self.tab(id) else {
            return Err(Message::Key("app.error.repository_unavailable"));
        };
        if self.start_repo_update(
            id,
            Some(edit),
            tab.history.limit,
            true,
            Some(Box::new(complete)),
            false,
            window,
            cx,
        ) {
            Ok(())
        } else {
            Err(Message::Key("app.error.repo_busy"))
        }
    }

    pub(super) fn refresh_watched_repo(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tab(id) else {
            return false;
        };
        self.start_repo_update(id, None, tab.history.limit, false, None, false, window, cx)
    }

    fn start_repo_update(
        &mut self,
        id: u64,
        edit: Option<RepoOperation>,
        limit: Option<usize>,
        report_operation: bool,
        complete: Option<Completion>,
        page_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return false;
        };
        let read_only = edit.is_none();
        let Some(generation) = tab.updates.begin(read_only, report_operation) else {
            return false;
        };
        tab.busy = !read_only;
        if !read_only && !matches!(edit, Some(RepoOperation::AutoFetch(_))) {
            tab.push_rejection = None;
        }
        tab.active_op = (!read_only).then(|| {
            edit.as_ref()
                .map_or(ActiveOp::Other, RepoOperation::activity)
        });
        if report_operation {
            tab.notice = None;
        }
        let repo = tab.repo.clone();
        let previous = tab.history.data.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                let result = edit.as_ref().map(|edit| edit.execute(&repo));
                // Watcher changes before this instant are covered by the
                // snapshot the reload below is about to read.
                let covered_at = Instant::now();
                let history = History::reload(&repo, limit, Some(&previous), page_only)
                    .map(|history| (history, limit));
                let reported =
                    if report_operation || result.as_ref().is_some_and(|result| result.is_err()) {
                        result.zip(edit)
                    } else {
                        None
                    };
                (reported, history, covered_at)
            },
            move |this, (result, history, covered_at), window, cx| {
                if !this
                    .tab_mut(id)
                    .is_some_and(|tab| tab.updates.finish(generation))
                {
                    return;
                }
                let succeeded = result.as_ref().is_some_and(|(result, _)| result.is_ok());
                this.finish_update(id, result, history, covered_at, window, cx);
                if let Some(complete) = complete {
                    complete(this, succeeded, window, cx);
                }
            },
        );
        if !read_only {
            cx.notify();
        }
        true
    }

    fn finish_update(
        &mut self,
        id: u64,
        result: Option<(git::Result<()>, RepoOperation)>,
        history: git::Result<(Arc<History>, Option<usize>)>,
        covered_at: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entering_conflict = result.as_ref().is_some_and(|(result, operation)| {
            result.is_err()
                && match operation {
                    RepoOperation::CommitGraph(prepared) => matches!(
                        prepared.operation,
                        git::CommitOperation::CherryPick { .. }
                            | git::CommitOperation::Revert { .. }
                            | git::CommitOperation::Merge
                            | git::CommitOperation::Rebase
                            | git::CommitOperation::InteractiveRebase(_)
                            | git::CommitOperation::Continue
                    ),
                    RepoOperation::Pop
                    | RepoOperation::ApplyStash(_)
                    | RepoOperation::StashFile(_)
                    | RepoOperation::PullRejected(_)
                    | RepoOperation::Pull { .. } => true,
                    _ => false,
                }
        });
        let conflict_file = if entering_conflict {
            history
                .as_ref()
                .ok()
                .and_then(|(history, _)| {
                    history
                        .snapshot
                        .status
                        .unstaged
                        .iter()
                        .find(|file| file.code == b'U')
                })
                .cloned()
        } else {
            None
        };
        if let Some((Ok(()), RepoOperation::CommitGraph(prepared))) = &result
            && let git::CommitOperation::CreateBranch { name, .. } = &prepared.operation
        {
            self.finish_inline_branch(id, name, window, cx);
        }
        let Some(tab) = self.tab_mut(id) else { return };
        let was_busy = tab.busy;
        if history.is_ok() {
            tab.watch_covered_at = Some(
                tab.watch_covered_at
                    .map_or(covered_at, |previous| previous.max(covered_at)),
            );
        }
        let was_paused = tab.history.data.snapshot.sequence.is_some();
        tab.busy = false;
        tab.active_op = None;
        let mut committed = false;
        let mut reset_selection = conflict_file.is_some();
        let mut finished_rebase = false;
        let mut notice = None;
        if let Some((result, edit)) = result {
            match result {
                Ok(()) => {
                    notice = edit.status_key().map(|key| Notice::info(Message::Key(key)));
                    committed = matches!(edit, RepoOperation::Commit(..));
                    finished_rebase = matches!(
                        &edit,
                        RepoOperation::CommitGraph(prepared)
                            if matches!(prepared.operation, git::CommitOperation::InteractiveRebase(_))
                    );
                    reset_selection = edit.resets_selection();
                    if committed {
                        tab.changes
                            .summary
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        tab.changes
                            .description
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
                Err(git::Error::PushRejected(request)) => {
                    tab.push_rejection = Some(*request);
                }
                Err(error) => {
                    notice = Some(if conflict_file.is_some() {
                        Notice::info(Message::Key("app.conflicts.paused"))
                    } else {
                        Notice::failure(error)
                    })
                }
            }
        }
        let mut changed = was_busy;
        match history {
            Ok((history, limit)) => {
                changed |= !Arc::ptr_eq(&tab.history.data, &history) || tab.history.limit != limit;
                tab.history.data = history;
                tab.history.limit = limit;
                tab.push_rejection = tab
                    .push_rejection
                    .take()
                    .filter(|request| request.matches_snapshot(&tab.history.data.snapshot));
                tab.changes
                    .selection
                    .refresh_worktree(&tab.history.data.snapshot.status);
            }
            Err(error) => {
                let message = if let Some(previous) = notice.take() {
                    Message::RefreshFailed {
                        previous: Box::new(previous.message),
                        error,
                    }
                } else {
                    error.into()
                };
                tab.notify(Notice::failure(message), window, cx);
                cx.notify();
                return;
            }
        }
        if let Some(mut session) = tab.rebase.take() {
            changed = true;
            if finished_rebase || tab.history.data.snapshot.sequence.is_some() {
                reset_selection = true;
            } else {
                let snapshot = &tab.history.data.snapshot;
                let same_checkout = snapshot.branch == session.context.branch
                    && snapshot.head.as_deref() == Some(session.context.head.as_str())
                    && snapshot.status.is_empty();
                let entries_valid = tab.history.data.position.contains_key(&session.plan.base)
                    && session
                        .plan
                        .entries
                        .iter()
                        .all(|entry| tab.history.data.position.contains_key(&entry.commit.id));
                if same_checkout && entries_valid {
                    session.reproject(&tab.history.data);
                    tab.rebase = Some(session);
                } else {
                    notice = Some(Notice::warning(Message::Key("app.commit_menu.stale")));
                    reset_selection = true;
                }
            }
        }
        if tab.history.data.snapshot.sequence.is_some() && notice.is_none() && !was_paused {
            notice = Some(Notice::info(Message::Key("app.commit_menu.paused")));
        }
        if let Some(notice) = notice {
            tab.notify(notice, window, cx);
        }
        let initial_selection = HistoryState::initial_selection(&tab.history.data);
        let leave_wip = tab.history.selected.is_none() && initial_selection.is_some();
        if committed || reset_selection || leave_wip {
            let selected = if committed {
                tab.history.data.snapshot.head.clone()
            } else {
                initial_selection
            };
            self.select(id, selected, Some(ScrollStrategy::Top), window, cx);
        } else if let Some(file) = tab.review.active.as_ref().map(|diff| diff.file.clone()) {
            let updated = match file.scope {
                Scope::Worktree | Scope::Index => {
                    let status = &tab.history.data.snapshot.status;
                    let candidates = if file.scope == Scope::Index {
                        [
                            (&status.staged, Scope::Index),
                            (&status.unstaged, Scope::Worktree),
                        ]
                    } else {
                        [
                            (&status.unstaged, Scope::Worktree),
                            (&status.staged, Scope::Index),
                        ]
                    };
                    candidates.into_iter().find_map(|(changes, scope)| {
                        changes
                            .iter()
                            .find(|change| change.path == file.change.path)
                            .cloned()
                            .map(|change| FileTarget { scope, change })
                    })
                }
                Scope::Commit { .. } => Some(file),
            };
            if let Some(file) = updated {
                self.refresh_diff(id, file, window, cx);
            } else {
                tab.review.close();
                changed = true;
            }
        } else if tab
            .history
            .selected
            .as_ref()
            .is_some_and(|oid| !tab.active_history().position.contains_key(oid))
        {
            self.select(id, initial_selection, None, window, cx);
        }
        if let Some(change) = conflict_file.filter(|_| self.session.active == Some(id)) {
            self.open_diff(
                id,
                FileTarget {
                    scope: Scope::Worktree,
                    change,
                },
                window,
                cx,
            );
        }
        self.resume_toolbar_branch(id, window, cx);
        if self
            .tab(id)
            .is_some_and(|tab| tab.history.wants_older)
        {
            self.load_older(id, window, cx);
        }
        if changed {
            cx.notify();
        }
    }
}
