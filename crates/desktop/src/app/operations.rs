use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{self as git, Change, FileTarget, Patch, Repo, Scope};

use super::{
    Client,
    changes::StageAction,
    history::{History, HistoryState},
};

#[derive(Clone)]
pub(super) enum RepoOperation {
    StageFiles(StageAction, Option<Change>),
    Hunk(FileTarget, Arc<Patch>, usize),
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
            Self::Hunk(file, patch, index) => repo.apply_hunk(file, patch, *index),
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

    fn label(&self) -> &'static str {
        match self {
            Self::StageFiles(StageAction::Stage, _) => "Changes staged",
            Self::StageFiles(StageAction::Unstage, _) => "Changes unstaged",
            Self::Hunk(file, ..) => match StageAction::for_scope(&file.scope) {
                Some(StageAction::Unstage) => "Hunk unstaged",
                _ => "Hunk staged",
            },
            Self::Commit(..) => "Commit created",
            Self::Switch(_) => "Branch checked out",
            Self::Branch(_) => "Branch created",
            Self::Fetch(_) => "Fetch completed",
            Self::Push { .. } => "Branch pushed",
            Self::Stash => "Changes stashed",
            Self::Pop => "Stash restored",
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
                    tab.notice = Some((false, edit.label().into()));
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
                Err(error) => tab.notice = Some((true, error.to_string())),
            }
        }
        match history {
            Ok((history, limit)) => {
                tab.history.data = history;
                tab.history.limit = limit;
            }
            Err(error) => {
                let message = tab
                    .notice
                    .as_ref()
                    .map_or(error.to_string(), |(_, previous)| {
                        format!("{previous}\nRefresh failed: {error}")
                    });
                tab.notice = Some((true, message));
                cx.notify();
                return;
            }
        }
        if committed || reset_selection {
            let selected = if committed {
                tab.history.data.snapshot.head.clone()
            } else {
                HistoryState::initial_selection(&tab.history.data)
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
            let selected = HistoryState::initial_selection(&tab.history.data);
            self.select(id, selected, None, window, cx);
        }
        cx.notify();
    }
}
