use crate::{Repo, Result, RuleError, command::run_write, protocol::text};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Upstream {
    pub remote: String,
    pub reference: String,
}

impl Upstream {
    pub(crate) fn tracking_name(&self) -> Option<String> {
        self.reference
            .strip_prefix("refs/heads/")
            .map(|branch| format!("{}/{branch}", self.remote))
    }
}

impl Repo {
    pub fn fetch(&self, remote: Option<&str>) -> Result<()> {
        self.fetch_remotes(remote, false)
    }

    pub fn auto_fetch(&self) -> Result<()> {
        self.fetch_remotes(None, true)
    }

    fn fetch_remotes(&self, remote: Option<&str>, unattended: bool) -> Result<()> {
        let mut git = self.git();
        if unattended {
            git.args(["-c", "credential.interactive=false"])
                .env("GCM_INTERACTIVE", "0");
        }
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
            return Err(RuleError::BranchChanged.into());
        }
        if !set_upstream && self.references(Some(branch))?.1.as_ref() != Some(upstream) {
            return Err(RuleError::UpstreamChanged.into());
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
        self.remote_summary().map(|(names, _)| names)
    }

    pub(crate) fn remote_summary(&self) -> Result<(Vec<String>, Option<String>)> {
        let bytes = self.read(&["remote", "--verbose"])?;
        let mut remotes = std::collections::BTreeMap::<String, Option<String>>::new();
        for line in text(&bytes).lines() {
            let Some((name, value)) = line.split_once('\t') else {
                continue;
            };
            let url = remotes.entry(name.to_owned()).or_default();
            if url.is_none()
                && let Some(value) = value.strip_suffix(" (fetch)")
            {
                *url = (!value.is_empty()).then(|| value.to_owned());
            }
        }
        let url = remotes
            .get("origin")
            .or_else(|| remotes.values().next())
            .cloned()
            .flatten();
        Ok((remotes.into_keys().collect(), url))
    }

    fn ensure_remote(&self, remote: &str) -> Result<()> {
        if self
            .remotes()?
            .iter()
            .any(|configured| configured == remote)
        {
            Ok(())
        } else {
            Err(RuleError::MissingRemote(remote.into()).into())
        }
    }
}
