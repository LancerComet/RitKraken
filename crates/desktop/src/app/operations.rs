use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{self as git, Change, FileTarget, Patch, PatchSelection, Repo, Scope};

use super::{
    Client, Message,
    changes::StageAction,
    history::{History, HistoryState},
};

#[derive(Clone)]
pub(super) enum RepoOperation {
    StageFiles(StageAction, Option<Change>),
    StagePatch(FileTarget, Arc<Patch>, PatchSelection),
    DiscardFile(Arc<git::DiscardFile>),
    IgnoreFile(Change, git::IgnoreRule, bool),
    StashFile(Change),
    Commit(String, String),
    Switch(String),
    Branch(String),
    Fetch(Option<String>),
    Push {
        branch: String,
        upstream: git::Upstream,
        set_upstream: bool,
    },
    Stash,
    Pop,
}

impl RepoOperation {
    fn execute(&self, repo: &Repo) -> git::Result<()> {
        match self {
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
            Self::Branch(branch) => repo.create_branch(branch),
            Self::Fetch(remote) => repo.fetch(remote.as_deref()),
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
            Self::Branch(_) => "app.notice.branched",
            Self::Fetch(_) => "app.notice.fetched",
            Self::Push { .. } => "app.notice.pushed",
            Self::Stash => "app.notice.stashed",
            Self::Pop => "app.notice.popped",
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
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        tab.notice = None;
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                let result = edit.as_ref().map(|edit| edit.execute(&repo));
                let history = History::load(&repo, limit).map(|history| (history, limit));
                (result.zip(edit), history)
            },
            move |this, (result, history), window, cx| {
                this.finish_update(id, result, history, window, cx);
            },
        );
        cx.notify();
    }

    fn finish_update(
        &mut self,
        id: u64,
        result: Option<(git::Result<()>, RepoOperation)>,
        history: git::Result<(Arc<History>, Option<usize>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.busy = false;
        let mut committed = false;
        let mut reset_selection = false;
        if let Some((result, edit)) = result {
            match result {
                Ok(()) => {
                    tab.notice = Some((false, Message::Key(edit.status_key())));
                    committed = matches!(edit, RepoOperation::Commit(..));
                    reset_selection = matches!(
                        edit,
                        RepoOperation::Switch(_)
                            | RepoOperation::Branch(_)
                            | RepoOperation::Stash
                            | RepoOperation::Pop
                    );
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
        cx.notify();
    }
}
