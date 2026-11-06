use crate::{Repo, Result, command::run_write, protocol::text};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upstream {
    pub remote: String,
    pub reference: String,
}

impl Repo {
    pub fn fetch(&self, remote: Option<&str>) -> Result<()> {
        let mut git = self.git();
        git.arg("fetch");
        if let Some(remote) = remote {
            self.ensure_remote(remote)?;
            git.arg("--").arg(remote);
        } else {
            git.arg("--all");
        }
        run_write(&mut git)
    }

    pub fn push(&self, branch: &str, upstream: &Upstream, set_upstream: bool) -> Result<()> {
        if self.checked_out_branch()?.as_deref() != Some(branch) {
            return Err(
                "The checked-out branch changed. Review the current branch before pushing.".into(),
            );
        }
        if !set_upstream && self.references(Some(branch))?.1.as_ref() != Some(upstream) {
            return Err("The upstream changed. Review the current upstream before pushing.".into());
        }
        self.ensure_remote(&upstream.remote)?;
        let mut git = self.git();
        git.args(["push", "--no-follow-tags"]);
        if set_upstream {
            git.arg("--set-upstream");
        }
        let refspec = format!("refs/heads/{branch}:{}", upstream.reference);
        run_write(git.arg("--").arg(&upstream.remote).arg(refspec))
    }

    pub(crate) fn remotes(&self) -> Result<Vec<String>> {
        self.read(&["remote"])
            .map(|bytes| text(&bytes).lines().map(str::to_owned).collect())
    }

    fn ensure_remote(&self, remote: &str) -> Result<()> {
        if self
            .remotes()?
            .iter()
            .any(|configured| configured == remote)
        {
            Ok(())
        } else {
            Err(format!("Remote {remote} is no longer configured.").into())
        }
    }
}
