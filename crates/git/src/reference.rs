use crate::{
    RefKind, Reference, Repo, Result, RuleError, Snapshot,
    command::{run_with_input, run_write},
};

#[derive(Clone, Debug)]
pub enum RefOperation {
    Checkout,
    Track(String),
    Rename(String),
    /// Explicit deletion after the caller has obtained confirmation.
    Delete,
}

/// Captured without I/O when the menu opens, and checked again at execution.
#[derive(Clone, Debug)]
pub struct RefRequest {
    pub reference: Reference,
    pub operation: RefOperation,
    pub head: Option<String>,
    pub branch: Option<String>,
}

impl RefRequest {
    pub fn checkout(snapshot: &Snapshot, reference: Reference) -> Self {
        let operation = if reference.kind == RefKind::Remote {
            let name = remote_branch(&reference.name, &snapshot.remotes)
                .map_or(reference.name.as_str(), |(_, name)| name)
                .to_owned();
            RefOperation::Track(name)
        } else {
            RefOperation::Checkout
        };
        Self::new(snapshot, reference, operation)
    }

    pub fn new(snapshot: &Snapshot, reference: Reference, operation: RefOperation) -> Self {
        Self {
            reference,
            operation,
            head: snapshot.head.clone(),
            branch: snapshot.branch.clone(),
        }
    }
}

impl Reference {
    pub fn full_name(&self) -> String {
        format!("refs/{}/{}", self.namespace(), self.name)
    }

    fn namespace(&self) -> &'static str {
        match self.kind {
            RefKind::Local => "heads",
            RefKind::Remote => "remotes",
            RefKind::Tag => "tags",
        }
    }
}

/// Remote names may themselves contain slashes; choose the longest exact prefix.
pub fn remote_branch<'a, 'b>(name: &'a str, remotes: &'b [String]) -> Option<(&'b str, &'a str)> {
    remotes
        .iter()
        .filter_map(|remote| {
            let branch = name.strip_prefix(remote)?.strip_prefix('/')?;
            (!branch.is_empty()).then_some((remote.as_str(), branch))
        })
        .max_by_key(|(remote, _)| remote.len())
}

impl Repo {
    pub fn apply_ref_operation(&self, request: &RefRequest) -> Result<()> {
        let reference = &request.reference;
        self.validate_ref_name(&reference.name, reference.namespace())?;
        if self.resolve_ref("HEAD")? != request.head || self.checked_out_branch()? != request.branch
        {
            return Err(RuleError::StaleReference.into());
        }
        if self.operation_state()?.kind.is_some() {
            return Err(RuleError::SequenceInProgress.into());
        }
        let (references, _) = self.references(request.branch.as_deref())?;
        let current = references
            .iter()
            .find(|current| current.kind == reference.kind && current.name == reference.name)
            .filter(|current| {
                current.object_id == reference.object_id && current.id == reference.id
            })
            .ok_or(RuleError::StaleReference)?;
        match &request.operation {
            RefOperation::Checkout => match current.kind {
                RefKind::Local if request.branch.as_deref() != Some(&current.name) => {
                    if current.checked_out {
                        return Err(RuleError::BranchInUse.into());
                    }
                    self.switch(&current.name)
                }
                RefKind::Tag => {
                    run_write(self.git().args(["switch", "--detach", &current.id, "--"]))
                }
                _ => Err(RuleError::RefUnavailable.into()),
            },
            RefOperation::Track(name) if current.kind == RefKind::Remote => {
                self.create_branch_at(name, None, true, Some(&current.name))
            }
            RefOperation::Rename(name) if current.kind != RefKind::Remote => {
                self.validate_ref_name(name, current.namespace())?;
                let destination = format!("refs/{}/{name}", current.namespace());
                if self.resolve_ref(&destination)?.is_some() {
                    return Err(RuleError::RefExists.into());
                }
                if current.kind == RefKind::Local {
                    if current.checked_out && request.branch.as_deref() != Some(&current.name) {
                        return Err(RuleError::BranchInUse.into());
                    }
                    // Git owns branch config, reflog and checked-out HEAD migration.
                    run_write(self.git().args(["branch", "-m", "--", &current.name, name]))
                } else {
                    // Keep the annotated/signed tag object intact. Both changes are atomic.
                    let transaction = format!(
                        "start\ncreate {destination} {}\ndelete {} {}\nprepare\ncommit\n",
                        current.object_id,
                        current.full_name(),
                        current.object_id,
                    );
                    run_with_input(
                        self.git().args(["update-ref", "--stdin"]),
                        &[transaction.as_bytes()],
                    )
                    .map(drop)
                }
            }
            RefOperation::Delete => match current.kind {
                RefKind::Local => {
                    if current.checked_out || request.branch.as_deref() == Some(&current.name) {
                        return Err(RuleError::BranchInUse.into());
                    }
                    // Deleting the selected branch does not depend on its relation
                    // to HEAD. Git still protects every checked-out worktree.
                    self.preserve_deleted_branch(&current.name, &current.id)?;
                    run_write(self.git().args(["branch", "-D", "--", &current.name]))
                }
                RefKind::Tag => run_write(self.git().args([
                    "update-ref",
                    "--no-deref",
                    "-d",
                    &current.full_name(),
                    &current.object_id,
                ])),
                RefKind::Remote => {
                    let remotes = self.remotes()?;
                    let (remote, branch) =
                        remote_branch(&current.name, &remotes).ok_or(RuleError::RefUnavailable)?;
                    let target = format!("refs/heads/{branch}");
                    // Reject deletion if the server branch moved after the displayed snapshot.
                    run_write(self.git().args([
                        "push",
                        "--no-follow-tags",
                        &format!("--force-with-lease={target}:{}", current.object_id),
                        "--",
                        remote,
                        &format!(":{target}"),
                    ]))
                }
            },
            _ => Err(RuleError::RefUnavailable.into()),
        }
    }
}
