#[cfg(test)]
extern crate self as ritkraken_git;

mod command;
mod commit_ops;
mod config;
mod diff;
mod error;
mod history;
mod protocol;
mod reference;
mod remotes;
mod repository;
mod watch;
mod worktree;

pub use commit_ops::{
    CommitAction, CommitAvailability, CommitBlock, CommitContext, CommitMenuSnapshot,
    CommitMenuState, CommitOperation, CommitRelation, GitEditor, MenuMetadata,
    PreparedCommitOperation, RebaseAction, RebaseEntry, RebasePlan, ResetMode, SequenceKind,
    run_git_editor,
};
pub use config::global_config_path;
pub use diff::{
    ExportedFile, FileTarget, PATCH_VIEW_LIMIT, Patch, PatchSelection, PatchSide, Scope,
    align_changed_lines, edge_whitespace_key,
};
pub use error::{Error, Result, RuleError};
pub use history::{
    BlameLine, Commit, CommitDetail, CommitPerson, FileCommit, FileHistory, RefKind, Reference,
    Snapshot, short_id,
};
pub use reference::{RefOperation, RefRequest, remote_branch};
pub use remotes::Upstream;
pub use repository::Repo;
pub use watch::RepoWatch;
pub use worktree::{Change, DiscardFile, IgnoreRule, Status};
