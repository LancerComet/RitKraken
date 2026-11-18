use std::{
    path::{Path, PathBuf},
    process::Output,
    thread::{self, Scope, ScopedJoinHandle},
};

use crate::{
    Commit, Error, Repo, Result, RuleError,
    command::{checked, read_output},
    history::{COMMIT_DATE_FORMAT, COMMIT_LOG_FORMAT, parse_log},
    protocol::text,
};

use super::{CommitContext, CommitMenuState, CommitRelation, SequenceKind, SequenceProgress};

pub(crate) struct OperationState {
    pub kind: Option<SequenceKind>,
    pub editing_head: Option<String>,
    pub progress: Option<SequenceProgress>,
    pub directory: PathBuf,
}

impl Repo {
    pub fn commit_at(&self, id: &str) -> Result<Commit> {
        ensure_object_id(id)?;
        parse_log(&self.read(&[
            "show",
            "--no-patch",
            "--no-notes",
            "--no-show-signature",
            "--encoding=UTF-8",
            COMMIT_DATE_FORMAT,
            COMMIT_LOG_FORMAT,
            "-z",
            id,
            "--",
        ])?)
        .into_iter()
        .next()
        .ok_or_else(|| RuleError::InvalidCommit.into())
    }

    pub fn commit_context(&self, id: &str) -> Result<CommitContext> {
        // Each Git process costs tens of milliseconds to start on Windows, so
        // independent reads run side by side. HEAD and the branch are read
        // again at the end to reject a checkout that moved meanwhile.
        ensure_object_id(id)?;
        thread::scope(|scope| {
            let commit = spawn(scope, || self.commit_at(id))?;
            let message = spawn(scope, || self.commit_message(id))?;
            let status = spawn(scope, || self.status())?;
            let remotes = spawn(scope, || self.remote_summary())?;
            let operation = spawn(scope, || self.operation_state())?;
            let references = spawn(scope, || {
                let branch = self.checked_out_branch()?;
                let references = self.references(branch.as_deref())?;
                Ok((branch, references))
            })?;
            let head = self.resolve_ref("HEAD")?.ok_or(RuleError::InvalidCommit)?;
            let staged_version = {
                let head = head.clone();
                spawn(scope, move || {
                    self.read(&[
                        "diff",
                        "--cached",
                        "--raw",
                        "--no-abbrev",
                        "--no-ext-diff",
                        "--no-textconv",
                        &head,
                        "--",
                    ])
                })?
            };
            let commit = join(commit)?;
            let relation = self.commit_relation(&commit.id, &head)?;
            let linear_rebase = match self.rebase_upstream(&head, &commit.id, relation)? {
                Some(upstream) => self
                    .read(&[
                        "rev-list",
                        "--max-count=1",
                        "--min-parents=2",
                        &format!("{upstream}..{head}"),
                        "--",
                    ])?
                    .is_empty(),
                None => false,
            };
            let status = join(status)?;
            let (branch, (references, upstream)) = join(references)?;
            let (remotes, remote_url) = join(remotes)?;
            let operation = join(operation)?;
            let message = join(message)?;
            let staged_version = join(staged_version)?;
            let conflicted = status.has_conflicts();
            let tracked_remote = upstream.as_ref().and_then(crate::Upstream::tracking_name);
            let editing_rebase_commit = operation.editing_head.as_deref() == Some(head.as_str());
            let current_branch = spawn(scope, || self.checked_out_branch())?;
            if self.resolve_ref("HEAD")?.as_deref() != Some(head.as_str())
                || join(current_branch)? != branch
            {
                return Err(RuleError::StaleCommit.into());
            }
            Ok(CommitContext {
                menu: CommitMenuState {
                    references: references
                        .into_iter()
                        .filter(|reference| reference.id == commit.id)
                        .collect(),
                    commit,
                    head,
                    branch,
                    relation,
                    linear_rebase,
                    remote_url,
                    dirty: !status.is_empty(),
                    conflicted,
                    staged_count: status.staged.len(),
                    sequence: operation.kind,
                    editing_rebase_commit,
                    remote_names: remotes,
                    tracked_remote,
                },
                message,
                staged_version,
            })
        })
    }

    /// The full message of a commit, as shown in its details.
    fn commit_message(&self, id: &str) -> Result<String> {
        let message = self.read(&[
            "show",
            "--no-patch",
            "--no-notes",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%B",
            id,
            "--",
        ])?;
        Ok(text(&message).trim_end().to_owned())
    }

    fn commit_relation(&self, commit: &str, head: &str) -> Result<CommitRelation> {
        Ok(if commit == head {
            CommitRelation::Head
        } else if self.is_ancestor(commit, head)? {
            CommitRelation::Ancestor
        } else if self.is_ancestor(head, commit)? {
            CommitRelation::Descendant
        } else if succeeds_or_absent(read_output(self.git().args(["merge-base", head, commit])))? {
            CommitRelation::Diverged
        } else {
            CommitRelation::Unrelated
        })
    }

    pub(crate) fn rebase_upstream(
        &self,
        head: &str,
        target: &str,
        relation: CommitRelation,
    ) -> Result<Option<String>> {
        match relation {
            CommitRelation::Ancestor => Ok(Some(target.to_owned())),
            CommitRelation::Diverged => {
                let output = read_output(self.git().args(["merge-base", "--all", head, target]))?;
                if output.status.code() == Some(1) {
                    return Ok(None);
                }
                let bases = text(&checked(output)?);
                let mut lines = bases.lines();
                let Some(first) = lines.next() else {
                    return Ok(None);
                };
                if lines.next().is_some() {
                    return Ok(None);
                }
                Ok(Some(first.to_owned()))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool> {
        succeeds_or_absent(read_output(self.git().args([
            "merge-base",
            "--is-ancestor",
            ancestor,
            descendant,
        ])))
    }

    pub fn sequence_kind(&self) -> Result<Option<SequenceKind>> {
        self.operation_state().map(|state| state.kind)
    }

    /// Pure file reads inside the worktree's own `.git` directory: every name
    /// checked here is per-worktree state.
    pub(crate) fn operation_state(&self) -> Result<OperationState> {
        let directory = self.git_dir().to_path_buf();
        let editing_head = read_trimmed(directory.join("rebase-merge").join("amend"));
        for (name, kind) in [
            ("rebase-merge", SequenceKind::Rebase),
            ("rebase-apply", SequenceKind::Rebase),
            ("MERGE_HEAD", SequenceKind::Merge),
            ("CHERRY_PICK_HEAD", SequenceKind::CherryPick),
            ("REVERT_HEAD", SequenceKind::Revert),
            ("BISECT_LOG", SequenceKind::Other),
        ] {
            let marker = directory.join(name);
            if marker.exists() {
                let progress =
                    (kind == SequenceKind::Rebase).then(|| read_rebase_progress(&marker));
                return Ok(OperationState {
                    kind: Some(kind),
                    directory,
                    editing_head,
                    progress,
                });
            }
        }
        let todo = directory.join("sequencer").join("todo");
        if let Ok(contents) = std::fs::read_to_string(todo) {
            let kind = if contents.trim_start().starts_with("revert ") {
                SequenceKind::Revert
            } else {
                SequenceKind::CherryPick
            };
            return Ok(OperationState {
                kind: Some(kind),
                directory,
                editing_head,
                progress: None,
            });
        }
        Ok(OperationState {
            kind: None,
            directory,
            editing_head,
            progress: None,
        })
    }
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn read_rebase_progress(directory: &Path) -> SequenceProgress {
    let branch = read_trimmed(directory.join("head-name"))
        .and_then(|name| name.strip_prefix("refs/heads/").map(str::to_owned));
    let (step_file, total_file) = if directory.join("msgnum").exists() {
        ("msgnum", "end")
    } else {
        ("next", "last")
    };
    SequenceProgress {
        current_step: read_trimmed(directory.join(step_file)).and_then(|s| s.parse().ok()),
        total_steps: read_trimmed(directory.join(total_file)).and_then(|s| s.parse().ok()),
        branch,
        onto: read_trimmed(directory.join("onto")),
    }
}

fn succeeds_or_absent(output: Result<Output>) -> Result<bool> {
    let output = output?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => checked(output).map(|_| false).map_err(Error::from),
    }
}

/// UI commands use immutable object IDs, never revisions which can move or be
/// interpreted as command-line options.
fn ensure_object_id(id: &str) -> Result<()> {
    if matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(RuleError::InvalidCommit.into())
    }
}

fn spawn<'scope, T: Send + 'scope>(
    scope: &'scope Scope<'scope, '_>,
    read: impl FnOnce() -> Result<T> + Send + 'scope,
) -> Result<ScopedJoinHandle<'scope, Result<T>>> {
    Ok(thread::Builder::new().spawn_scoped(scope, read)?)
}

fn join<T>(reader: ScopedJoinHandle<'_, Result<T>>) -> Result<T> {
    reader.join().expect("Git reader panicked")
}
