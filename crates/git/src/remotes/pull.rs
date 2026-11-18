use super::Upstream;
use crate::{
    Repo, Result, RuleError,
    command::{join, run_write, spawn},
};

/// How a pull integrates the upstream branch into the checked-out branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullMode {
    /// Fast-forward when possible, otherwise create a merge commit.
    FastForwardOrMerge,
    /// Fast-forward, or stop without changing the branch.
    FastForwardOnly,
    /// Replay local commits on top of the upstream branch.
    Rebase,
}

impl Repo {
    pub fn pull(&self, branch: &str, upstream: &Upstream, mode: PullMode) -> Result<()> {
        if self.checked_out_branch()?.as_deref() != Some(branch) {
            return Err(RuleError::BranchChanged.into());
        }
        std::thread::scope(|scope| -> Result<()> {
            let references = spawn(scope, || self.references(Some(branch)))?;
            let remote = spawn(scope, || self.ensure_remote(&upstream.remote))?;
            if join(references)?.1.as_ref() != Some(upstream) {
                return Err(RuleError::UpstreamChanged.into());
            }
            join(remote)?;
            Ok(())
        })?;
        if self.operation_state()?.kind.is_some() {
            return Err(RuleError::SequenceInProgress.into());
        }
        self.run_pull(upstream, mode)
    }

    /// Runs `git pull` with an explicit strategy so `pull.rebase` and
    /// `pull.ff` settings cannot silently change the selected behavior.
    /// Working files are never stashed implicitly.
    pub(super) fn run_pull(&self, upstream: &Upstream, mode: PullMode) -> Result<()> {
        let mut git = self.git();
        git.arg("pull");
        git.args(match mode {
            PullMode::FastForwardOrMerge => ["--no-rebase", "--ff", "--no-edit"].as_slice(),
            PullMode::FastForwardOnly => &["--no-rebase", "--ff-only"],
            PullMode::Rebase => &["--rebase"],
        });
        run_write(git.args([
            "--no-autostash",
            "--",
            &upstream.remote,
            &upstream.reference,
        ]))
    }
}
