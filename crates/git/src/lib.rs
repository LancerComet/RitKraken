#[cfg(test)]
extern crate self as ritkraken_git;

mod command;
mod commit_ops;
mod config;
mod conflicts;
mod diff;
mod error;
mod history;
mod protocol;
mod recovery;
mod reference;
mod remotes;
mod repository;
mod watch;
mod worktree;
mod worktrees;

pub use command::{GitCall, set_git_call_listener};
pub use commit_ops::{
    CommitAction, CommitAvailability, CommitBlock, CommitContext, CommitMenuSnapshot,
    CommitMenuState, CommitOperation, CommitRelation, GitEditor, MenuMetadata,
    PreparedCommitOperation, RebaseAction, RebaseEntry, RebasePlan, ResetMode, SequenceKind,
    SequenceProgress, compose_squash_message, run_git_editor,
};
pub use config::global_config_path;
pub use conflicts::{ConflictFile, ConflictResolution, ConflictVersion, has_conflict_markers};
pub use diff::{
    ExportedFile, FileTarget, PATCH_VIEW_LIMIT, Patch, PatchSelection, PatchSide, Scope,
    align_changed_lines, edge_whitespace_key,
};
pub use error::{Error, Result, RuleError};
pub use history::{
    BlameLine, Commit, CommitDetail, CommitPerson, CommitSearch, FileCommit, FileHistory, RefKind,
    Reference, Snapshot, short_id,
};
pub use recovery::{RecoveryHistory, ReflogEntry};
pub use reference::{RefOperation, RefRequest, remote_branch};
pub use remotes::{DEFAULT_AUTO_FETCH_TIMEOUT, NewRemote, PullMode, PushRejection, Upstream};
pub use repository::Repo;
pub use watch::RepoWatch;
pub use worktree::{Change, DiscardFile, IgnoreRule, Stash, Status};
pub use worktrees::{NewWorktree, Worktree};
