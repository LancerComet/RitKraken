use ritkraken_git::{Error, RuleError, Snapshot, short_id};

use crate::i18n::tr;

pub(super) enum Message {
    Key(&'static str),
    Detail { key: &'static str, detail: String },
    Git(Error),
    RefreshFailed { previous: Box<Self>, error: Error },
}

impl Message {
    pub(super) fn detail(key: &'static str, detail: impl ToString) -> Self {
        Self::Detail {
            key,
            detail: detail.to_string(),
        }
    }

    pub(super) fn text(&self) -> String {
        match self {
            Self::Key(key) => tr!(*key),
            Self::Detail { key, detail } => tr!(*key, detail = detail),
            Self::Git(error) => git_error(error),
            Self::RefreshFailed { previous, error } => tr!(
                "app.error.refresh_after_operation",
                previous = previous.text(),
                detail = git_error(error)
            ),
        }
    }
}

impl From<Error> for Message {
    fn from(error: Error) -> Self {
        Self::Git(error)
    }
}

fn git_error(error: &Error) -> String {
    match error {
        Error::MissingGit(_) => tr!("app.error.missing_git"),
        Error::Invocation(error) => tr!("app.error.run_git", detail = error),
        Error::Timeout(duration) => tr!("app.error.git_timeout", seconds = duration.as_secs()),
        Error::Git { message, .. } => tr!("app.error.git_failed", detail = message),
        Error::Exited(status) => tr!("app.error.git_exited", status = status),
        Error::PreviewRead(status) => tr!("app.error.preview_read", status = status),
        Error::Io(error) => tr!("app.error.io", detail = error),
        Error::InvalidOperation(detail) => tr!("app.error.operation", detail = detail),
        Error::Rule(rule) => rule_error(rule),
        Error::CleanupFailed { operation, cleanup } => tr!(
            "app.error.cleanup_failed",
            detail = git_error(operation),
            cleanup = git_error(cleanup)
        ),
    }
}

fn rule_error(rule: &RuleError) -> String {
    let key = match rule {
        RuleError::StaleReference => "app.ref_menu.stale",
        RuleError::BranchInUse => "app.ref_menu.in_use",
        RuleError::RefExists => "app.ref_menu.exists",
        RuleError::RefUnavailable => "app.ref_menu.unavailable",
        RuleError::InvalidCommit => "app.commit_menu.invalid_commit",
        RuleError::StaleCommit => "app.commit_menu.stale",
        RuleError::CommitUnavailable => "app.commit_menu.unavailable",
        RuleError::DirtyWorktree => "app.commit_menu.dirty",
        RuleError::UnresolvedConflicts => "app.commit_menu.conflict",
        RuleError::SequenceInProgress => "app.commit_menu.in_progress",
        RuleError::InvalidRefName => "app.commit_menu.invalid_name",
        RuleError::MainlineRequired => "app.commit_menu.mainline_required",
        RuleError::InvalidRebasePlan => "app.commit_menu.invalid_plan",
        RuleError::StaleFile => "app.file_menu.stale_file",
        RuleError::InvalidFilePath => "app.file_menu.invalid_path",
        RuleError::InvalidIgnorePattern => "app.file_menu.invalid_ignore_pattern",
        RuleError::InvalidIgnoreFile => "app.file_menu.invalid_ignore_file",
        RuleError::ReadOnlyFile => "app.file_menu.read_only",
        RuleError::BlamePreviewLimit => "app.file_menu.blame_limit",
        RuleError::StashListChanged => "app.file_menu.stash_list_changed",
        RuleError::DeletedFile => "app.error.deleted_file",
        RuleError::NoFileContents => "app.error.no_file_contents",
        RuleError::FilePreviewLimit => "app.error.file_preview_limit",
        RuleError::BinaryFile => "app.error.binary_file",
        RuleError::InvalidFileEncoding => "app.error.file_encoding",
        RuleError::StalePatch => "app.error.stale_patch",
        RuleError::UnstageableSelection => "app.error.unstageable_selection",
        RuleError::BranchChanged => "app.error.branch_changed",
        RuleError::UpstreamChanged => "app.error.upstream_changed",
        RuleError::EmptyCommitSummary => "app.error.commit_summary",
        RuleError::InvalidRemoteName => "app.remotes.invalid_name",
        RuleError::EmptyRemoteUrl => "app.remotes.pull_required",
        RuleError::InvalidRemoteUrl => "app.remotes.invalid_url",
        RuleError::MissingRemote(remote) => {
            return tr!("app.error.missing_remote", remote = remote);
        }
    };
    tr!(key)
}

pub(super) fn branch_label(snapshot: &Snapshot) -> String {
    match (snapshot.branch.as_ref(), snapshot.head.as_deref()) {
        (Some(branch), _) => branch.clone(),
        (None, Some(head)) => tr!("app.branch.detached", sha = short_id(head)),
        (None, None) => "HEAD".into(),
    }
}
