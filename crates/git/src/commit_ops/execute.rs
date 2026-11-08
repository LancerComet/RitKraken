use crate::{
    Repo, Result, RuleError,
    command::{run_read, run_write},
    worktree::WorkspaceVersion,
};

use super::{CommitContext, CommitOperation, CommitRelation, GitEditor, ResetMode, SequenceKind};

/// Captures the repository state reviewed in the confirmation dialog. The
/// operation is checked again immediately before Git is allowed to mutate it.
#[derive(Clone, Debug)]
pub struct PreparedCommitOperation {
    pub context: CommitContext,
    pub operation: CommitOperation,
    pub affected_count: usize,
    pub published: Vec<String>,
    workspace: Option<WorkspaceVersion>,
    references: Option<Vec<crate::Reference>>,
}

impl Repo {
    pub fn prepare_commit_operation(
        &self,
        expected: &CommitContext,
        operation: CommitOperation,
    ) -> Result<PreparedCommitOperation> {
        let context = self.validate_commit_operation(expected, &operation)?;
        let workspace = matches!(
            operation,
            CommitOperation::Reset(ResetMode::Mixed | ResetMode::Hard)
                | CommitOperation::Amend { .. }
        )
        .then(|| self.workspace_version())
        .transpose()?;
        let rewrites = matches!(
            operation,
            CommitOperation::Amend { .. }
                | CommitOperation::Reset(_)
                | CommitOperation::Rebase
                | CommitOperation::InteractiveRebase(_)
        );
        let affected_count = if matches!(operation, CommitOperation::Amend { .. }) {
            1
        } else if rewrites {
            crate::protocol::text(&self.read(&[
                "rev-list",
                "--count",
                &format!("{}..{}", context.commit.id, context.head),
                "--",
            ])?)
            .trim()
            .parse()
            .map_err(|_| RuleError::InvalidCommit)?
        } else {
            0
        };
        let mut published = Vec::new();
        let references = rewrites
            .then(|| self.references(None).map(|(references, _)| references))
            .transpose()?;
        if rewrites {
            for reference in references
                .as_ref()
                .into_iter()
                .flatten()
                .filter(|reference| reference.kind == crate::RefKind::Remote)
            {
                let included = if matches!(operation, CommitOperation::Amend { .. }) {
                    self.is_ancestor(&context.head, &reference.id)?
                } else {
                    let common = crate::command::read_output(self.git().args([
                        "merge-base",
                        "--all",
                        &context.head,
                        &reference.id,
                    ]))?;
                    if common.status.code() == Some(1) {
                        continue;
                    }
                    let bases = crate::protocol::text(&crate::command::checked(common)?);
                    let mut included = false;
                    for base in bases.lines() {
                        included |= !self.is_ancestor(base, &context.commit.id)?;
                    }
                    included
                };
                if included {
                    published.push(reference.name.clone());
                }
            }
        }
        Ok(PreparedCommitOperation {
            context,
            operation,
            workspace,
            references,
            affected_count,
            published,
        })
    }

    pub fn apply_commit_operation(
        &self,
        expected: &PreparedCommitOperation,
        editor: &GitEditor,
    ) -> Result<()> {
        let context = self.validate_commit_operation(&expected.context, &expected.operation)?;
        if let Some(references) = &expected.references
            && self.references(None)?.0 != *references
        {
            return Err(RuleError::StaleCommit.into());
        }
        if let Some(workspace) = &expected.workspace
            && self.workspace_version()? != *workspace
        {
            return Err(RuleError::StaleCommit.into());
        }
        let target = &context.commit.id;
        let mut git = self.git();
        editor.configure(self, &mut git)?;
        match &expected.operation {
            CommitOperation::CreateBranch {
                name,
                checkout,
                track,
            } => {
                return self.create_branch_at(name, Some(target), *checkout, track.as_deref());
            }
            CommitOperation::CreateTag(name) => {
                git.args(["tag", "--", name, target]);
            }
            CommitOperation::Switch(branch) => return self.switch(branch),
            CommitOperation::Checkout => {
                git.args(["switch", "--detach", "--no-recurse-submodules", target]);
            }
            CommitOperation::CherryPick { mainline } | CommitOperation::Revert { mainline } => {
                git.arg(
                    if matches!(expected.operation, CommitOperation::CherryPick { .. }) {
                        "cherry-pick"
                    } else {
                        "revert"
                    },
                )
                .arg("--no-edit");
                if let Some(parent) = mainline {
                    git.args(["--mainline", &parent.to_string()]);
                }
                git.arg(target);
            }
            CommitOperation::Merge => {
                git.args(["merge", "--no-edit", "--no-stat", "--no-autostash"]);
                git.arg(if context.relation == CommitRelation::Descendant {
                    "--ff-only"
                } else {
                    "--ff"
                });
                git.arg(target);
            }
            CommitOperation::Rebase => {
                // Older Git versions ignore this config, while newer versions
                // honor it without requiring the newer --no-update-refs flag.
                git.args([
                    "-c",
                    "rebase.updateRefs=false",
                    "rebase",
                    "--rebase-merges",
                    "--no-autostash",
                    "--no-fork-point",
                    target,
                ]);
            }
            CommitOperation::InteractiveRebase(plan) => {
                return self.start_interactive_rebase(plan, editor);
            }
            CommitOperation::Amend {
                message,
                include_staged,
            } => {
                let mut input = tempfile::NamedTempFile::new()?;
                std::io::Write::write_all(&mut input, message.as_bytes())?;
                git.args(["commit", "--amend", "--allow-empty"]);
                if !include_staged {
                    git.arg("--only");
                }
                return run_write(git.arg("--file").arg(input.path()));
            }
            CommitOperation::Reset(mode) => {
                git.args([
                    "reset",
                    "--no-recurse-submodules",
                    match mode {
                        ResetMode::Soft => "--soft",
                        ResetMode::Mixed => "--mixed",
                        ResetMode::Hard => "--hard",
                    },
                    target,
                    "--",
                ]);
            }
            CommitOperation::Continue | CommitOperation::Skip | CommitOperation::Abort => {
                let sequence = context.sequence.ok_or(RuleError::CommitUnavailable)?;
                let command = match sequence {
                    SequenceKind::Merge => "merge",
                    SequenceKind::Rebase => "rebase",
                    SequenceKind::CherryPick => "cherry-pick",
                    SequenceKind::Revert => "revert",
                    SequenceKind::Other => return Err(RuleError::CommitUnavailable.into()),
                };
                git.args([
                    command,
                    match expected.operation {
                        CommitOperation::Continue => "--continue",
                        CommitOperation::Skip => "--skip",
                        _ => "--abort",
                    },
                ]);
            }
        }
        let result = run_write(&mut git);
        let finishing_rebase = context.sequence == Some(SequenceKind::Rebase)
            && matches!(
                expected.operation,
                CommitOperation::Continue | CommitOperation::Skip | CommitOperation::Abort
            );
        if finishing_rebase
            && self
                .sequence_kind()
                .is_ok_and(|sequence| sequence.is_none())
        {
            // Cleanup must not replace Git's result after history has already
            // changed, and unrelated operations never own these files.
            let _ = self.clean_rebase_files();
        }
        result
    }

    fn validate_commit_operation(
        &self,
        expected: &CommitContext,
        operation: &CommitOperation,
    ) -> Result<CommitContext> {
        let fresh = self.commit_context(&expected.commit.id)?;
        if !fresh.same_checkout(expected) {
            return Err(RuleError::StaleCommit.into());
        }
        if matches!(
            operation,
            CommitOperation::Amend {
                include_staged: true,
                ..
            }
        ) && fresh.staged_version != expected.staged_version
        {
            return Err(RuleError::StaleCommit.into());
        }
        if let CommitOperation::Switch(branch) = operation
            && !fresh.references.iter().any(|reference| {
                reference.kind == crate::RefKind::Local && &reference.name == branch
            })
        {
            return Err(RuleError::StaleCommit.into());
        }
        fresh.ensure_available(operation.action())?;
        match operation {
            CommitOperation::CreateBranch { name, track, .. } => {
                self.validate_ref_name(name, "heads")?;
                if let Some(track) = track
                    && !fresh.references.iter().any(|reference| {
                        reference.kind == crate::RefKind::Remote && &reference.name == track
                    })
                {
                    return Err(RuleError::StaleCommit.into());
                }
            }
            CommitOperation::CreateTag(name) => self.validate_ref_name(name, "tags")?,
            CommitOperation::CherryPick { mainline } | CommitOperation::Revert { mainline } => {
                self.validate_mainline(&fresh, *mainline)?;
            }
            CommitOperation::Amend { message, .. } if message.trim().is_empty() => {
                return Err(RuleError::EmptyCommitSummary.into());
            }
            CommitOperation::InteractiveRebase(plan) => self.validate_rebase_plan(&fresh, plan)?,
            _ => {}
        }
        Ok(fresh)
    }

    pub(crate) fn validate_mainline(
        &self,
        context: &CommitContext,
        mainline: Option<usize>,
    ) -> Result<()> {
        if if context.commit.parents.len() > 1 {
            !mainline.is_some_and(|parent| parent > 0 && parent <= context.commit.parents.len())
        } else {
            mainline.is_some()
        } {
            return Err(RuleError::MainlineRequired.into());
        }
        Ok(())
    }

    pub(crate) fn validate_ref_name(&self, name: &str, kind: &str) -> Result<()> {
        if name.is_empty()
            || name.starts_with('-')
            || name == "HEAD"
            || name.contains(['\0', '\n', '\r'])
        {
            return Err(RuleError::InvalidRefName.into());
        }
        run_read(
            self.git()
                .args(["check-ref-format", &format!("refs/{kind}/{name}")]),
        )
        .map(|_| ())
        .map_err(|_| RuleError::InvalidRefName.into())
    }

    pub(crate) fn create_branch_at(
        &self,
        name: &str,
        target: Option<&str>,
        checkout: bool,
        track: Option<&str>,
    ) -> Result<()> {
        self.validate_ref_name(name, "heads")?;
        let mut git = self.git();
        if checkout {
            git.args(["switch", "--no-guess", "-c", name]);
        } else {
            git.arg("branch");
        }
        if track.is_some() {
            git.arg("--track=direct");
        }
        if !checkout {
            git.args(["--", name]);
        }
        if let Some(target) = track.or(target) {
            git.arg(if track.is_some() {
                format!("refs/remotes/{target}")
            } else {
                target.to_owned()
            });
        }
        run_write(&mut git)
    }
}
