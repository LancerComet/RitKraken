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
        Error::Git { message, .. } => tr!("app.error.git_failed", detail = message),
        Error::Exited(status) => tr!("app.error.git_exited", status = status),
        Error::PreviewRead(status) => tr!("app.error.preview_read", status = status),
        Error::Io(error) => tr!("app.error.io", detail = error),
        Error::InvalidOperation(detail) => tr!("app.error.operation", detail = detail),
        Error::Rule(rule) => rule_error(rule),
    }
}

fn rule_error(rule: &RuleError) -> String {
    let key = match rule {
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
