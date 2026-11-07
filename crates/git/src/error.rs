use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git was not found. Install Git and ensure it is available on PATH.")]
    MissingGit(#[source] io::Error),
    #[error("Could not run Git: {0}")]
    Invocation(#[source] io::Error),
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
}

#[derive(Debug, thiserror::Error)]
pub enum RuleError {
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
