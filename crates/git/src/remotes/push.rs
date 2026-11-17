use std::process::Command;

use super::{PullMode, Upstream};
use crate::{
    Error, Repo, Result, RuleError, Snapshot,
    command::{checked, run_output, run_write},
    protocol::text,
};

/// The exact checkout, destination, and server tip reviewed after a rejected push.
#[derive(Clone, Debug)]
pub struct PushRejection {
    pub branch: String,
    pub upstream: Upstream,
    head: String,
    previous_upstream: Option<Upstream>,
    set_upstream: bool,
    push_url: String,
    remote_tip: String,
}

impl PushRejection {
    pub fn matches_snapshot(&self, snapshot: &Snapshot) -> bool {
        snapshot.branch.as_deref() == Some(&self.branch)
            && snapshot.head.as_deref() == Some(&self.head)
            && snapshot.upstream == self.previous_upstream
    }
}

impl Repo {
    pub fn push(&self, branch: &str, upstream: &Upstream, set_upstream: bool) -> Result<()> {
        self.validate_push(branch, upstream, set_upstream)?;
        let head = self.resolve_ref("HEAD")?.ok_or(RuleError::InvalidCommit)?;
        let previous_upstream = self.references(Some(branch))?.1;
        let urls = self.push_urls(&upstream.remote)?;
        let mut git = self.push_command(set_upstream);
        let refspec = format!("refs/heads/{branch}:{}", upstream.reference);
        let output = run_output(git.arg("--").arg(&upstream.remote).arg(&refspec))?;
        if !output.status.success()
            && is_non_fast_forward(&text(&output.stdout), &refspec)
            && urls.len() == 1
            && let Ok(remote_tip) = self.remote_push_tip(&urls[0], &upstream.reference)
        {
            return Err(Error::PushRejected(Box::new(PushRejection {
                branch: branch.to_owned(),
                upstream: upstream.clone(),
                head,
                previous_upstream,
                set_upstream,
                push_url: urls[0].clone(),
                remote_tip,
            })));
        }
        checked(output).map(drop)
    }

    pub fn force_push(&self, request: &PushRejection) -> Result<()> {
        self.validate_push_rejection(request)?;
        let mut git = self.push_command(false);
        git.arg(format!(
            "--force-with-lease={}:{}",
            request.upstream.reference, request.remote_tip
        ));
        // Push the reviewed object even if another process moves the local
        // branch after validation while the transport is starting.
        let refspec = format!("{}:{}", request.head, request.upstream.reference);
        run_write(git.arg("--").arg(&request.upstream.remote).arg(refspec))?;
        if request.set_upstream {
            for (key, value) in [
                ("remote", &request.upstream.remote),
                ("merge", &request.upstream.reference),
            ] {
                run_write(self.git().args([
                    "config",
                    "--local",
                    "--replace-all",
                    &format!("branch.{}.{key}", request.branch),
                    value,
                ]))?;
            }
        }
        Ok(())
    }

    pub fn pull_after_rejected_push(&self, request: &PushRejection) -> Result<()> {
        self.validate_push_rejection(request)?;
        // The Pull action integrates remote history without rewriting it.
        self.run_pull(&request.upstream, PullMode::FastForwardOrMerge)
    }

    fn validate_push(&self, branch: &str, upstream: &Upstream, set_upstream: bool) -> Result<()> {
        if self.checked_out_branch()?.as_deref() != Some(branch) {
            return Err(RuleError::BranchChanged.into());
        }
        if !set_upstream && self.references(Some(branch))?.1.as_ref() != Some(upstream) {
            return Err(RuleError::UpstreamChanged.into());
        }
        self.ensure_remote(&upstream.remote)
    }

    fn validate_push_rejection(&self, request: &PushRejection) -> Result<()> {
        self.validate_push(&request.branch, &request.upstream, request.set_upstream)?;
        if self.resolve_ref("HEAD")?.as_ref() != Some(&request.head) {
            return Err(RuleError::BranchChanged.into());
        }
        if self.references(Some(&request.branch))?.1 != request.previous_upstream
            || self.push_urls(&request.upstream.remote)? != [request.push_url.clone()]
        {
            return Err(RuleError::UpstreamChanged.into());
        }
        if self.operation_state()?.kind.is_some() {
            return Err(RuleError::SequenceInProgress.into());
        }
        Ok(())
    }

    fn push_command(&self, set_upstream: bool) -> Command {
        let mut git = self.git();
        git.args(["push", "--porcelain", "--no-follow-tags"]);
        if set_upstream {
            git.arg("--set-upstream");
        }
        git
    }

    fn push_urls(&self, remote: &str) -> Result<Vec<String>> {
        Ok(
            text(&self.read(&["remote", "get-url", "--push", "--all", "--", remote])?)
                .lines()
                .map(str::to_owned)
                .collect(),
        )
    }

    fn remote_push_tip(&self, url: &str, reference: &str) -> Result<String> {
        let output = self.read(&["ls-remote", "--refs", "--", url, reference])?;
        text(&output)
            .lines()
            .find_map(|line| {
                let (oid, name) = line.split_once('\t')?;
                (name == reference
                    && matches!(oid.len(), 40 | 64)
                    && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then(|| oid.to_owned())
            })
            .ok_or_else(|| RuleError::StaleReference.into())
    }
}

fn is_non_fast_forward(output: &str, refspec: &str) -> bool {
    output.lines().any(|line| {
        let mut fields = line.split('\t');
        fields.next() == Some("!")
            && fields.next() == Some(refspec)
            && matches!(
                fields.next(),
                Some(
                    "[rejected] (non-fast-forward)"
                        | "[rejected] (fetch first)"
                        | "[rejected] (needs force)"
                )
            )
    })
}
