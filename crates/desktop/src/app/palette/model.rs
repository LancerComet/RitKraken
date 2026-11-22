use ritkraken_git::{FileTarget, RefRequest, Snapshot};

use crate::{app::RepoTab, i18n::tr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum Command {
    OpenRepos,
    Preferences,
    Refresh,
    SearchCommits,
    Recovery,
    Worktrees,
    CreateBranch,
    Fetch,
    Push,
    StageAll,
    UnstageAll,
    Stash,
    Pop,
    Workspace,
    Compare,
}

impl Command {
    pub(super) const ALL: [Self; 15] = [
        Self::OpenRepos,
        Self::Preferences,
        Self::Refresh,
        Self::SearchCommits,
        Self::Recovery,
        Self::Worktrees,
        Self::CreateBranch,
        Self::Fetch,
        Self::Push,
        Self::StageAll,
        Self::UnstageAll,
        Self::Stash,
        Self::Pop,
        Self::Workspace,
        Self::Compare,
    ];

    pub(super) fn label(self) -> String {
        tr!(match self {
            Self::OpenRepos => "app.repositories.open_manager",
            Self::Preferences => "app.preferences.title",
            Self::Refresh => "app.palette.refresh",
            Self::SearchCommits => "app.search.title",
            Self::Recovery => "app.recovery.title",
            Self::Worktrees => "app.worktrees.title",
            Self::CreateBranch => "app.toolbar.create_branch_hint",
            Self::Fetch => "app.toolbar.fetch",
            Self::Push => "app.toolbar.push",
            Self::StageAll => "app.stage.stage_all",
            Self::UnstageAll => "app.stage.unstage_all",
            Self::Stash => "app.toolbar.stash",
            Self::Pop => "app.toolbar.pop",
            Self::Workspace => "app.palette.workspace",
            Self::Compare => "app.compare.palette",
        })
    }

    pub(in crate::app) fn available(self, context: Option<CommandContext<'_>>) -> bool {
        match self {
            Self::OpenRepos | Self::Preferences => true,
            Self::Refresh
            | Self::SearchCommits
            | Self::Recovery
            | Self::Worktrees
            | Self::CreateBranch
            | Self::Fetch
            | Self::Push
            | Self::StageAll
            | Self::UnstageAll
            | Self::Stash
            | Self::Pop
            | Self::Workspace
            | Self::Compare => {
                let Some(context) = context else { return false };
                let snapshot = context.snapshot;
                let available = !context.busy && !context.rebase;
                match self {
                    Self::Refresh => !context.busy,
                    Self::SearchCommits | Self::Recovery | Self::Worktrees | Self::Workspace => {
                        available
                    }
                    Self::CreateBranch => available && snapshot.head.is_some(),
                    Self::Fetch => available && !snapshot.remotes.is_empty(),
                    Self::Push => {
                        available
                            && snapshot.branch.is_some()
                            && snapshot.refs.iter().any(|r| snapshot.is_current_branch(r))
                            && snapshot
                                .upstream
                                .as_ref()
                                .is_some_and(|upstream| snapshot.remotes.contains(&upstream.remote))
                    }
                    Self::StageAll => {
                        available
                            && !snapshot.status.unstaged.is_empty()
                            && !snapshot.status.has_conflicts()
                    }
                    Self::UnstageAll => available && !snapshot.status.staged.is_empty(),
                    Self::Stash => {
                        available
                            && snapshot.head.is_some()
                            && !snapshot.status.is_empty()
                            && !snapshot.status.unstaged.iter().any(|c| c.code == b'U')
                    }
                    Self::Pop => available && snapshot.has_stash,
                    // Comparison is read-only, so it survives busy and rebase.
                    Self::Compare => snapshot.head.is_some(),
                    Self::OpenRepos | Self::Preferences => true,
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) struct CommandContext<'a> {
    pub snapshot: &'a Snapshot,
    pub busy: bool,
    pub rebase: bool,
}

impl<'a> From<&'a RepoTab> for CommandContext<'a> {
    fn from(tab: &'a RepoTab) -> Self {
        Self {
            snapshot: &tab.history.data.snapshot,
            busy: tab.busy,
            rebase: tab.rebase.is_some(),
        }
    }
}

#[derive(Clone)]
pub(super) enum Target {
    Command(Command),
    Branch(RefRequest),
    Commit(String),
    File(FileTarget),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Command,
    Branch,
    Commit,
    File,
}

impl Kind {
    pub(super) fn label(self) -> String {
        tr!(match self {
            Self::Command => "app.palette.command",
            Self::Branch => "app.palette.branch",
            Self::Commit => "app.palette.commit",
            Self::File => "app.palette.file",
        })
    }
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub title: String,
    pub details: String,
    pub target: Target,
}

impl Candidate {
    pub(super) fn kind(&self) -> Kind {
        match &self.target {
            Target::Command(_) => Kind::Command,
            Target::Branch(_) => Kind::Branch,
            Target::Commit(_) => Kind::Commit,
            Target::File(_) => Kind::File,
        }
    }
}

pub(super) fn filter(candidates: &[Candidate], query: &str) -> Vec<usize> {
    let query = query.trim();
    let (kind, query) = match query.chars().next() {
        Some('>') => (Some(Kind::Command), &query[1..]),
        Some('@') => (Some(Kind::Branch), &query[1..]),
        Some('#') => (Some(Kind::Commit), &query[1..]),
        Some('/') => (Some(Kind::File), &query[1..]),
        _ => (None, query),
    };
    let tokens: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if kind.is_some_and(|kind| kind != candidate.kind()) {
                return None;
            }
            let text = format!("{} {}", candidate.title, candidate.details).to_lowercase();
            tokens
                .iter()
                .all(|token| text.contains(token))
                .then_some(index)
        })
        .collect()
}
