use std::{io, process::ExitStatus, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git was not found. Install Git and ensure it is available on PATH.")]
    MissingGit(#[source] io::Error),
    #[error("Could not run Git: {0}")]
    Invocation(#[source] io::Error),
    #[error("Git operation timed out after {0:?}.")]
    Timeout(Duration),
    #[error("{message}")]
    Git { status: ExitStatus, message: String },
    #[error("Git exited with {0}.")]
    Exited(ExitStatus),
    #[error("Git could not read this preview ({0}).")]
    PreviewRead(ExitStatus),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Rule(#[from] RuleError),
    #[error("{0}")]
    InvalidOperation(String),
    #[error("{operation}\nAutomatic cleanup also failed: {cleanup}")]
    CleanupFailed {
        operation: Box<Error>,
        cleanup: Box<Error>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    #[error("This reference changed. Reopen its menu before continuing.")]
    StaleReference,
    #[error("This branch is checked out in a worktree and cannot be changed here.")]
    BranchInUse,
    #[error("This branch has commits not merged into HEAD. Merge it before deleting.")]
    UnmergedBranch,
    #[error("A reference with that name already exists.")]
    RefExists,
    #[error("This operation is not available for this reference.")]
    RefUnavailable,
    #[error("This commit is no longer valid.")]
    InvalidCommit,
    #[error("The repository changed. Reopen the commit menu and review the operation.")]
    StaleCommit,
    #[error("This operation is not available for the selected commit.")]
    CommitUnavailable,
    #[error("Commit or stash your changes before continuing.")]
    DirtyWorktree,
    #[error("Resolve and stage the conflicts before continuing.")]
    UnresolvedConflicts,
    #[error("Finish or abort the current Git operation first.")]
    SequenceInProgress,
    #[error("Enter a valid branch or tag name.")]
    InvalidRefName,
    #[error("Choose a mainline parent for this merge commit.")]
    MainlineRequired,
    #[error(
        "The rebase plan is invalid. Each commit must appear once and squash/fixup must follow a kept commit."
    )]
    InvalidRebasePlan,
    #[error("This file changed on disk. Review it again before continuing.")]
    StaleFile,
    #[error("This operation requires a repository-relative file path.")]
    InvalidFilePath,
    #[error("This file name cannot be represented by a Git ignore rule.")]
    InvalidIgnorePattern,
    #[error("The repository's .gitignore must be a regular file.")]
    InvalidIgnoreFile,
    #[error("Committed files are read-only.")]
    ReadOnlyFile,
    #[error("Blame previews are limited to 100,000 lines.")]
    BlamePreviewLimit,
    #[error("The file was restored, but the stash list changed. The stash was kept.")]
    StashListChanged,
    #[error("This file was deleted in the selected version.")]
    DeletedFile,
    #[error("This entry does not have text file contents.")]
    NoFileContents,
    #[error("This file exceeds the 8 MiB preview limit.")]
    FilePreviewLimit,
    #[error("Binary file contents cannot be previewed as text.")]
    BinaryFile,
    #[error("This file does not contain UTF-8 text.")]
    InvalidFileEncoding,
    #[error("The diff changed on disk. Review the refreshed diff before staging.")]
    StalePatch,
    #[error("This selection cannot be staged or unstaged separately.")]
    UnstageableSelection,
    #[error("The checked-out branch changed. Review the current branch before pushing.")]
    BranchChanged,
    #[error("The upstream changed. Review the current upstream before pushing.")]
    UpstreamChanged,
    #[error("Remote {0} is no longer configured.")]
    MissingRemote(String),
    #[error("Enter a valid remote name.")]
    InvalidRemoteName,
    #[error("Enter a pull URL.")]
    EmptyRemoteUrl,
    #[error("Enter a valid remote URL.")]
    InvalidRemoteUrl,
    #[error("Enter a commit summary.")]
    EmptyCommitSummary,
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::InvalidOperation(message.to_owned())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::InvalidOperation(message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
