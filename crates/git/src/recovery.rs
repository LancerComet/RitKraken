mod log;

use crate::{Repo, Result, RuleError, command::run_write};

#[derive(Clone, Debug)]
pub struct ReflogEntry {
    pub id: String,
    pub selector: String,
    pub date: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct RecoveryHistory {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub entries: Vec<ReflogEntry>,
    /// Previous distinct HEAD OID, including checkout movements. Restoring it
    /// moves the current branch tip; it never switches to the previous branch.
    pub undo_target: Option<String>,
}

impl Repo {
    pub fn recovery_history(&self) -> Result<RecoveryHistory> {
        let head = self.resolve_ref("HEAD")?;
        let branch = self.checked_out_branch()?;
        let mut entries = log::read_log(&self.git_dir().join("logs").join("HEAD"), "HEAD")?;
        let undo_target = entries
            .iter()
            .find(|entry| Some(&entry.id) != head.as_ref())
            .map(|entry| entry.id.clone());
        entries.extend(log::read_log(&self.deleted_branch_log()?, "deleted")?);
        Ok(RecoveryHistory {
            head,
            branch,
            entries,
            undo_target,
        })
    }

    /// Move only HEAD (and its current branch) using a soft reset. Staged,
    /// working, and untracked files remain untouched. Native reflog records the
    /// displaced HEAD for redo. This does not reverse a checkout or a sequence.
    pub fn restore_head(&self, history: &RecoveryHistory, target: &str) -> Result<()> {
        self.validate_recovery(history, target)?;
        run_write(
            self.git()
                .env("GIT_REFLOG_ACTION", "ritkraken: restore HEAD")
                .args(["reset", "--soft", target, "--"]),
        )
    }

    /// Create a new local branch without changing checkout or existing refs.
    pub fn recover_branch(
        &self,
        history: &RecoveryHistory,
        target: &str,
        name: &str,
    ) -> Result<()> {
        self.validate_recovery(history, target)?;
        let exists = std::thread::scope(|scope| -> Result<bool> {
            let name_check = std::thread::Builder::new()
                .spawn_scoped(scope, || self.validate_ref_name(name, "heads"))?;
            let exists = self.resolve_ref(&format!("refs/heads/{name}"))?.is_some();
            name_check.join().expect("name checker panicked")?;
            Ok(exists)
        })?;
        if exists {
            return Err(RuleError::RefExists.into());
        }
        self.run_create_branch(name, Some(target), false, None)
    }

    fn validate_recovery(&self, history: &RecoveryHistory, target: &str) -> Result<()> {
        if !history.entries.iter().any(|entry| entry.id == target) {
            return Err(RuleError::InvalidCommit.into());
        }
        std::thread::scope(|scope| -> Result<()> {
            let unmerged = std::thread::Builder::new()
                .spawn_scoped(scope, || self.read(&["ls-files", "--unmerged", "-z"]))?;
            let commit =
                std::thread::Builder::new().spawn_scoped(scope, || self.commit_at(target))?;
            if self.resolve_ref("HEAD")? != history.head
                || self.checked_out_branch()? != history.branch
            {
                return Err(RuleError::StaleCommit.into());
            }
            if self.operation_state()?.kind.is_some() {
                return Err(RuleError::SequenceInProgress.into());
            }
            if !unmerged
                .join()
                .expect("unmerged reader panicked")?
                .is_empty()
            {
                return Err(RuleError::UnresolvedConflicts.into());
            }
            if commit.join().expect("commit reader panicked")?.id != target {
                return Err(RuleError::InvalidCommit.into());
            }
            Ok(())
        })
    }
}
