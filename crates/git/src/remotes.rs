use std::time::Duration;

use crate::{
    Error, Repo, Result, RuleError,
    command::{read_output, run_write, run_write_with_timeout},
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

    /// The configured fetch URL and explicit push URL of a remote.
    pub fn remote_urls(&self, name: &str) -> Result<(String, Option<String>)> {
        self.ensure_remote(name)?;
        let pull = text(&self.read(&["remote", "get-url", "--", name])?)
            .trim()
            .to_owned();
        Ok((pull, self.configured_push_urls(name)?.into_iter().next()))
    }

    /// Rewrites a remote's URLs and renames it when the name changed; tracking
    /// references move with the rename.
    pub fn update_remote(&self, name: &str, remote: &NewRemote) -> Result<()> {
        self.ensure_remote(name)?;
        if remote.name != name
            && self
                .remotes()?
                .iter()
                .any(|configured| configured == &remote.name)
        {
            return Err(RuleError::RemoteExists.into());
        }
        run_write(
            self.git()
                .args(["remote", "set-url", "--", name, &remote.pull_url]),
        )?;
        for url in self.configured_push_urls(name)? {
            run_write(self.git().args([
                "remote",
                "set-url",
                "--push",
                "--delete",
                "--",
                name,
                &url,
            ]))?;
        }
        if let Some(push_url) = &remote.push_url {
            run_write(self.git().args([
                "remote",
                "set-url",
                "--push",
                "--",
                name,
                push_url,
            ]))?;
        }
        if remote.name != name {
            run_write(self.git().args(["remote", "rename", name, &remote.name]))?;
        }
        Ok(())
    }

    /// Removes a remote and its tracking references; branch upstreams left
    /// pointing at it become stale as with plain Git.
    pub fn remove_remote(&self, name: &str) -> Result<()> {
        self.ensure_remote(name)?;
        run_write(self.git().args(["remote", "remove", name]))
    }

    /// Explicit `remote.<name>.pushurl` values; `config --get-all` exits
    /// nonzero when the key is unset, which is not an error.
    fn configured_push_urls(&self, name: &str) -> Result<Vec<String>> {
        let output = read_output(self.git().args([
            "config",
            "--get-all",
            &format!("remote.{name}.pushurl"),
        ]))?;
        Ok(if output.status.success() {
            text(&output.stdout)
                .lines()
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        })
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

    pub(crate) fn ensure_remote(&self, remote: &str) -> Result<()> {
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
mod pull;
mod push;

pub use pull::PullMode;
pub use push::PushRejection;
