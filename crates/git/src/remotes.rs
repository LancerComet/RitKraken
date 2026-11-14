use std::time::Duration;

use crate::{
    Error, Repo, Result, RuleError,
    command::{run_write, run_write_with_timeout},
    protocol::text,
};

pub const DEFAULT_AUTO_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewRemote {
    name: String,
    pull_url: String,
    push_url: Option<String>,
}

impl NewRemote {
    pub fn new(name: &str, pull_url: &str, push_url: Option<&str>) -> Result<Self> {
        let name = name.trim();
        let pull_url = pull_url.trim();
        let push_url = push_url.map(str::trim).filter(|url| !url.is_empty());
        if name.is_empty() || name.contains('\0') {
            return Err(RuleError::InvalidRemoteName.into());
        }
        if pull_url.is_empty() {
            return Err(RuleError::EmptyRemoteUrl.into());
        }
        if pull_url.contains('\0') || push_url.is_some_and(|url| url.contains('\0')) {
            return Err(RuleError::InvalidRemoteUrl.into());
        }
        Ok(Self {
            name: name.to_owned(),
            pull_url: pull_url.to_owned(),
            push_url: push_url.map(str::to_owned),
        })
    }
}

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
    pub fn add_remote(&self, remote: &NewRemote) -> Result<()> {
        run_write(
            self.git()
                .args(["remote", "add", "--", &remote.name, &remote.pull_url]),
        )?;
        if let Some(push_url) = &remote.push_url
            && let Err(operation) = run_write(self.git().args([
                "remote",
                "set-url",
                "--push",
                "--",
                &remote.name,
                push_url,
            ]))
        {
            return match run_write(self.git().args(["remote", "remove", "--", &remote.name])) {
                Ok(()) => Err(operation),
                Err(cleanup) => Err(Error::CleanupFailed {
                    operation: Box::new(operation),
                    cleanup: Box::new(cleanup),
                }),
            };
        }
        Ok(())
    }

    pub fn fetch(&self, remote: Option<&str>) -> Result<()> {
        self.fetch_remotes(remote, None)
    }

    pub fn auto_fetch(&self) -> Result<()> {
        self.auto_fetch_with_timeout(DEFAULT_AUTO_FETCH_TIMEOUT)
    }

    pub fn auto_fetch_with_timeout(&self, timeout: Duration) -> Result<()> {
        self.fetch_remotes(None, Some(timeout))
    }

    fn fetch_remotes(&self, remote: Option<&str>, timeout: Option<Duration>) -> Result<()> {
        let mut git = self.git();
        if timeout.is_some() {
            git.args(["-c", "credential.interactive=false"])
                .env("GCM_INTERACTIVE", "0");
        }
        git.args(["fetch", "--prune"]);
        if let Some(remote) = remote {
            self.ensure_remote(remote)?;
            git.arg("--").arg(remote);
        } else {
            git.arg("--all");
        }
        if let Some(timeout) = timeout {
            run_write_with_timeout(git, timeout)
        } else {
            run_write(&mut git)
        }
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
