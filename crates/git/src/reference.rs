use std::thread;

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
    /// Publishes a local tag to the named remote without overwriting a
    /// different remote tag of the same name.
    PushTag(String),
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
    /// Reads run side by side: each Git process costs tens of milliseconds to
    /// start on Windows, and the name check, reference list, and remote list
    /// are independent.
    pub fn apply_ref_operation(&self, request: &RefRequest) -> Result<()> {
        let reference = &request.reference;
        // Existing names came from Git's own reference listing and are valid
        // by construction; only names typed into dialogs need checking. Track
        // always creates a local branch while Rename keeps the source kind.
        let (new_name, new_namespace) = match &request.operation {
            RefOperation::Track(name) => (Some(name.as_str()), "heads"),
            RefOperation::Rename(name) => (Some(name.as_str()), reference.namespace()),
            _ => (None, ""),
        };
        let remotes_needed = matches!(
            (reference.kind, &request.operation),
            (RefKind::Remote, RefOperation::Delete) | (RefKind::Tag, RefOperation::PushTag(_))
        );
        let (references, remotes) = thread::scope(|scope| -> Result<_> {
            let references =
                thread::Builder::new().spawn_scoped(scope, || self.references(None))?;
            let name_check = new_name
                .map(|name| {
                    thread::Builder::new()
                        .spawn_scoped(scope, || self.validate_ref_name(name, new_namespace))
                })
                .transpose()?;
            let remotes = remotes_needed
                .then(|| thread::Builder::new().spawn_scoped(scope, || self.remotes()))
                .transpose()?;
            let branch = self.checked_out_branch()?;
            let operation = self.operation_state()?;
            let (references, _) = references.join().expect("references reader panicked")?;
            name_check
                .map(|check| check.join().expect("name checker panicked"))
                .transpose()?;
            if self.head_oid(&references)? != request.head || branch != request.branch {
                return Err(RuleError::StaleReference.into());
            }
            if operation.kind.is_some() {
                return Err(RuleError::SequenceInProgress.into());
            }
            let remotes = remotes
                .map(|reader| reader.join().expect("remotes reader panicked"))
                .transpose()?;
            Ok((references, remotes))
        })?;
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
                self.run_create_branch(name, None, true, Some(&current.name))
            }
            RefOperation::Rename(name) if current.kind != RefKind::Remote => {
                let destination = format!("refs/{}/{name}", current.namespace());
                if references
                    .iter()
                    .any(|reference| reference.full_name() == destination)
                {
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
            RefOperation::PushTag(remote) if current.kind == RefKind::Tag => {
                if !remotes
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|configured| configured == remote)
                {
                    return Err(RuleError::MissingRemote(remote.clone()).into());
                }
                // Push the reviewed tag object, not whatever the name resolves
                // to by the time the transport starts.
                run_write(self.git().args([
                    "push",
                    "--no-follow-tags",
                    "--",
                    remote,
                    &format!("{}:{}", current.object_id, current.full_name()),
                ]))
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
                    let (remote, branch) =
                        remote_branch(&current.name, remotes.as_deref().unwrap_or_default())
                            .ok_or(RuleError::RefUnavailable)?;
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
