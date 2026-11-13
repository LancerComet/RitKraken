mod auto_fetch;
mod confirm;

use std::{sync::Arc, time::Duration};

use gpui_kit::*;
use ritkraken_git::{self as git, Change, FileTarget, Patch, PatchSelection, Repo, Scope};

use super::{
    Client, Message,
    changes::StageAction,
    history::{History, HistoryState},
    session::ActiveOp,
};

type Completion = Box<dyn FnOnce(&mut Client, bool, &mut Window, &mut Context<Client>)>;

#[derive(Clone)]
pub(super) enum RepoOperation {
    Reference(git::RefRequest),
    CommitGraph(Arc<git::PreparedCommitOperation>),
    StageFiles(StageAction, Option<Change>),
    StagePatch(FileTarget, Arc<Patch>, PatchSelection),
    DiscardFile(Arc<git::DiscardFile>),
    IgnoreFile(Change, git::IgnoreRule, bool),
    StashFile(Change),
    Commit(String, String),
    Switch(String),
    Fetch(Option<String>),
    AddRemote(git::NewRemote),
    AutoFetch(Duration),
    Push {
        branch: String,
        upstream: git::Upstream,
        set_upstream: bool,
    },
    Stash,
    Pop,
}

impl RepoOperation {
    fn resets_selection(&self) -> bool {
        match self {
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
            Self::Switch(_) | Self::Stash | Self::Pop => true,
            _ => false,
        }
    }

    fn execute(&self, repo: &Repo) -> git::Result<()> {
        match self {
            Self::Reference(request) => repo.apply_ref_operation(request),
            Self::CommitGraph(operation) => repo.apply_commit_operation(
                operation,
                &git::GitEditor::for_executable(&std::env::current_exe()?),
            ),
            Self::StageFiles(StageAction::Stage, change) => repo.stage(change.as_ref()),
            Self::StageFiles(StageAction::Unstage, change) => repo.unstage(change.as_ref()),
            Self::StagePatch(file, patch, selection) => repo.apply_partial(file, patch, *selection),
            Self::DiscardFile(expected) => repo.discard_file(expected),
            Self::IgnoreFile(change, rule, stop_tracking) => {
                repo.ignore_file(change, *rule)?;
                if *stop_tracking {
                    repo.stop_tracking(change)?;
                }
                Ok(())
            }
            Self::StashFile(change) => repo.stash_file(change),
            Self::Commit(summary, description) => repo.commit(summary, description),
            Self::Switch(branch) => repo.switch(branch),
            Self::Fetch(remote) => repo.fetch(remote.as_deref()),
            Self::AddRemote(remote) => repo.add_remote(remote),
            Self::AutoFetch(timeout) => repo.auto_fetch_with_timeout(*timeout),
            Self::Push {
                branch,
                upstream,
                set_upstream,
            } => repo.push(branch, upstream, *set_upstream),
            Self::Stash => repo.stash(),
            Self::Pop => repo.pop_stash(),
        }
    }

    fn status_key(&self) -> &'static str {
        match self {
            Self::Reference(_) => "app.ref_menu.completed",
            Self::CommitGraph(_) => "app.commit_menu.completed",
            Self::StageFiles(StageAction::Stage, _) => "app.notice.staged",
            Self::StageFiles(StageAction::Unstage, _) => "app.notice.unstaged",
            Self::DiscardFile(_) => "app.file_menu.discarded",
            Self::IgnoreFile(_, _, false) => "app.file_menu.ignored",
            Self::IgnoreFile(_, _, true) => "app.file_menu.stopped_tracking",
            Self::StashFile(_) => "app.notice.stashed",
            Self::StagePatch(file, _, selection) => match (file.scope == Scope::Index, selection) {
                (false, PatchSelection::Hunk(_)) => "app.notice.hunk_staged",
                (true, PatchSelection::Hunk(_)) => "app.notice.hunk_unstaged",
                (false, PatchSelection::Line { .. }) => "app.notice.line_staged",
                (true, PatchSelection::Line { .. }) => "app.notice.line_unstaged",
            },
            Self::Commit(..) => "app.notice.committed",
            Self::Switch(_) => "app.notice.switched",
            Self::Fetch(_) | Self::AutoFetch(_) => "app.notice.fetched",
            Self::AddRemote(_) => "app.notice.remote_added",
            Self::Push { .. } => "app.notice.pushed",
            Self::Stash => "app.notice.stashed",
            Self::Pop => "app.notice.popped",
        }
    }

    fn activity(&self) -> ActiveOp {
        match self {
            Self::Fetch(_) | Self::AutoFetch(_) => ActiveOp::Fetch,
            Self::Push { .. } => ActiveOp::Push,
            Self::Commit(..) => ActiveOp::Commit,
            Self::Stash => ActiveOp::Stash,
            Self::Pop => ActiveOp::Pop,
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
        self.start_repo_update(id, edit, limit, true, None, window, cx);
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
    ) {
        let Some(tab) = self.tab(id) else { return };
        self.start_repo_update(id, None, tab.history.limit, false, None, window, cx);
    }

    fn start_repo_update(
        &mut self,
        id: u64,
        edit: Option<RepoOperation>,
        limit: Option<usize>,
        report_operation: bool,
        complete: Option<Completion>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return false;
        };
        tab.busy = true;
        tab.active_op = Some(
            edit.as_ref()
                .map_or(ActiveOp::Other, RepoOperation::activity),
        );
        if report_operation {
            tab.notice = None;
        }
        let repo = tab.repo.clone();
        let previous_menu = tab.history.data.menu.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                let result = edit.as_ref().map(|edit| edit.execute(&repo));
                let history = History::reload(&repo, limit, Some(&previous_menu))
                    .map(|history| (history, limit));
                let reported = if report_operation {
                    result.zip(edit)
                } else {
                    None
                };
                (reported, history)
            },
            move |this, (result, history), window, cx| {
                let succeeded = result.as_ref().is_some_and(|(result, _)| result.is_ok());
                this.finish_update(id, result, history, window, cx);
                if let Some(complete) = complete {
                    complete(this, succeeded, window, cx);
                }
            },
        );
        cx.notify();
        true
    }

    fn finish_update(
        &mut self,
        id: u64,
        result: Option<(git::Result<()>, RepoOperation)>,
        history: git::Result<(Arc<History>, Option<usize>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((Ok(()), RepoOperation::CommitGraph(prepared))) = &result
            && let git::CommitOperation::CreateBranch { name, .. } = &prepared.operation
        {
            self.finish_inline_branch(id, name, window, cx);
        }
        let Some(tab) = self.tab_mut(id) else { return };
        tab.busy = false;
        tab.active_op = None;
        let mut committed = false;
        let mut reset_selection = false;
        if let Some((result, edit)) = result {
            match result {
                Ok(()) => {
                    tab.notice = Some((false, Message::Key(edit.status_key())));
                    committed = matches!(edit, RepoOperation::Commit(..));
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
                Err(error) => tab.notice = Some((true, error.into())),
            }
        }
        match history {
            Ok((history, limit)) => {
                tab.history.data = history;
                tab.history.limit = limit;
            }
            Err(error) => {
                let message = if let Some((_, previous)) = tab.notice.take() {
                    Message::RefreshFailed {
                        previous: Box::new(previous),
                        error,
                    }
                } else {
                    error.into()
                };
                tab.notice = Some((true, message));
                cx.notify();
                return;
            }
        }
        if tab.history.data.snapshot.sequence.is_some()
            && !tab.notice.as_ref().is_some_and(|(error, _)| *error)
        {
            tab.notice = Some((false, Message::Key("app.commit_menu.paused")));
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
                self.open_diff(id, file, window, cx);
            } else {
                tab.review.close();
            }
        } else if tab
            .history
            .selected
            .as_ref()
            .is_some_and(|oid| !tab.history.data.position.contains_key(oid))
        {
            self.select(id, initial_selection, None, window, cx);
        }
        self.resume_toolbar_branch(id, window, cx);
        cx.notify();
    }
}
