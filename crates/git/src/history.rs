mod detail;
mod file;
mod search;

use std::{collections::HashSet, thread};

use crate::{
    Change, Error, Repo, Result, Status, Upstream,
    protocol::{path_from_bytes, text},
};

pub use detail::{CommitDetail, CommitPerson};
pub use file::{BlameLine, FileCommit, FileHistory};
pub use search::CommitSearch;

pub(crate) const COMMIT_DATE_FORMAT: &str = "--date=format-local:%Y/%m/%d @ %H:%M";
/// Lists commits newest first by commit time while still showing every child
/// before its parents. `--topo-order` would instead keep each line of history
/// together, placing older commits of one branch above newer ones of another.
pub(crate) const HISTORY_ORDER: &str = "--date-order";
pub(crate) const COMMIT_LOG_FORMAT: &str = "--format=%H%x00%P%x00%an%x00%ae%x00%ad%x00%s";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub author_email: String,
    pub date: String,
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reference {
    pub kind: RefKind,
    pub name: String,
    pub id: String,
    /// The actual ref object, before peeling an annotated tag.
    pub object_id: String,
    /// Checked out in any worktree, including a linked worktree.
    pub checked_out: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub status: Status,
    pub refs: Vec<Reference>,
    pub remotes: Vec<String>,
    pub upstream: Option<Upstream>,
    pub has_stash: bool,
    /// Stashes, newest first; their commits are also listed in `commits`.
    pub stashes: Vec<crate::Stash>,
    pub sequence: Option<crate::SequenceKind>,
    pub menu_metadata: crate::MenuMetadata,
    pub commits: Vec<Commit>,
    pub history_limited: bool,
}

impl Snapshot {
    pub fn refs_of_kind(&self, kind: RefKind) -> impl Iterator<Item = &Reference> {
        self.refs
            .iter()
            .filter(move |reference| reference.kind == kind)
    }

    pub fn is_current_branch(&self, reference: &Reference) -> bool {
        reference.kind == RefKind::Local && self.branch.as_deref() == Some(reference.name.as_str())
    }

    pub fn sequence_progress(&self) -> Option<&crate::SequenceProgress> {
        self.menu_metadata.progress.as_ref()
    }

    pub fn is_editing_rebase_commit(&self) -> bool {
        self.head
            .as_deref()
            .is_some_and(|head| self.menu_metadata.editing_head.as_deref() == Some(head))
    }
}

impl Repo {
    pub fn snapshot(&self, history_limit: Option<usize>) -> Result<Snapshot> {
        thread::scope(|scope| {
            let history = thread::Builder::new().spawn_scoped(scope, || {
                let stashes = self.stashes()?;
                let (commits, limited) = self.history_commits(history_limit, &stashes)?;
                Ok::<_, Error>((commits, limited, stashes))
            })?;
            let status = thread::Builder::new().spawn_scoped(scope, || self.status())?;
            let references = thread::Builder::new().spawn_scoped(scope, || {
                let branch = self.checked_out_branch()?;
                let (refs, upstream, reference_version) =
                    self.reference_state(branch.as_deref())?;
                let head = self.head_oid(&refs)?;
                let operation = self.operation_state()?;
                let version = crate::commit_ops::menu::GraphVersion::read(
                    reference_version,
                    &operation.directory,
                )?;
                Ok::<_, Error>((branch, refs, upstream, head, operation, version))
            })?;
            let (remotes, remote_url) = self.remote_summary()?;
            let status = status.join().expect("status reader panicked")?;
            let (branch, refs, upstream, head, operation, version) =
                references.join().expect("reference reader panicked")?;
            let (commits, history_limited, stashes) =
                history.join().expect("history reader panicked")?;
            Ok(Snapshot {
                branch,
                head,
                status,
                refs,
                remotes,
                upstream,
                has_stash: !stashes.is_empty(),
                stashes,
                sequence: operation.kind,
                menu_metadata: crate::MenuMetadata {
                    version,
                    remote_url,
                    editing_head: operation.editing_head,
                    progress: operation.progress,
                },
                commits,
                history_limited,
            })
        })
    }

    /// Grows only the commit list of a previous snapshot. Status, references,
    /// and other fields are carried over: a page expansion is display-only and
    /// the next refresh re-reads them anyway.
    pub fn snapshot_page(
        &self,
        previous: &Snapshot,
        history_limit: Option<usize>,
    ) -> Result<Snapshot> {
        let (commits, history_limited) =
            self.history_commits(history_limit, &previous.stashes)?;
        Ok(Snapshot {
            branch: previous.branch.clone(),
            head: previous.head.clone(),
            status: previous.status.clone(),
            refs: previous.refs.clone(),
            remotes: previous.remotes.clone(),
            upstream: previous.upstream.clone(),
            has_stash: previous.has_stash,
            stashes: previous.stashes.clone(),
            sequence: previous.sequence,
            menu_metadata: previous.menu_metadata.clone(),
            commits,
            history_limited,
        })
    }

    fn history_commits(
        &self,
        history_limit: Option<usize>,
        stashes: &[crate::Stash],
    ) -> Result<(Vec<Commit>, bool)> {
        let mut args = vec![
            "log",
            "--exclude=refs/stash",
            "--all",
            HISTORY_ORDER,
            COMMIT_DATE_FORMAT,
            "--encoding=UTF-8",
            COMMIT_LOG_FORMAT,
            "-z",
        ];
        let limit = history_limit.map(|limit| format!("--max-count={}", limit.saturating_add(1)));
        if let Some(limit) = limit.as_deref() {
            args.push(limit);
        }
        // During detached checkouts and an Edit pause, HEAD can point to a
        // commit not yet reachable from any named reference.
        args.extend(["--ignore-missing", "HEAD"]);
        // Listing stash commits lets --date-order place each stash by time,
        // above the commit it was made on.
        args.extend(stashes.iter().map(|stash| stash.id.as_str()));
        args.push("--");
        let mut commits = parse_log(&self.read(&args)?);
        let history_limited = history_limit.is_some_and(|limit| commits.len() > limit);
        if let Some(limit) = history_limit {
            commits.truncate(limit);
        }
        attach_stashes(&mut commits, stashes);
        Ok((commits, history_limited))
    }

    pub fn committed_files(&self, id: &str) -> Result<Vec<Change>> {
        let bytes = self.read(&["ls-tree", "-r", "--name-only", "-z", id])?;
        Ok(bytes
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| Change {
                path: path_from_bytes(path),
                previous: None,
                code: b' ',
            })
            .collect())
    }

    /// `.git/HEAD` as a commit id without running Git: the checked-out
    /// branch's tip, the detached object id, or `None` for an unborn HEAD.
    pub(crate) fn head_oid(&self, references: &[Reference]) -> Result<Option<String>> {
        let head = self.head_file()?;
        Ok(match head.strip_prefix("ref: ") {
            Some(target) => references
                .iter()
                .find(|reference| reference.full_name() == target)
                .map(|reference| reference.id.clone()),
            None => (!head.is_empty()).then_some(head),
        })
    }

    pub(crate) fn references(
        &self,
        branch: Option<&str>,
    ) -> Result<(Vec<Reference>, Option<Upstream>)> {
        self.reference_state(branch)
            .map(|(references, upstream, _)| (references, upstream))
    }

    fn reference_state(
        &self,
        branch: Option<&str>,
    ) -> Result<(Vec<Reference>, Option<Upstream>, Vec<u8>)> {
        let bytes = self.read(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(symref)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(if)%(worktreepath)%(then)1%(else)0%(end)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
            "refs/replace",
        ])?;
        let mut upstream = None;
        let refs = bytes
            .split(|byte| *byte == b'\n')
            .filter_map(|line| {
                let mut fields = line.split(|byte| *byte == 0);
                let name = std::str::from_utf8(fields.next()?).ok()?;
                let oid = fields.next()?;
                let peeled = fields.next()?;
                if !fields.next()?.is_empty() {
                    return None;
                }
                let (kind, name) = if let Some(name) = name.strip_prefix("refs/heads/") {
                    (RefKind::Local, name)
                } else if let Some(name) = name.strip_prefix("refs/remotes/") {
                    (RefKind::Remote, name)
                } else {
                    (RefKind::Tag, name.strip_prefix("refs/tags/")?)
                };
                let remote = fields.next()?;
                let reference = fields.next()?;
                if kind == RefKind::Local && Some(name) == branch {
                    if !remote.is_empty() && reference.starts_with(b"refs/heads/") {
                        upstream = Some(Upstream {
                            remote: text(remote),
                            reference: text(reference),
                        });
                    }
                }
                let id = text(if peeled.is_empty() { oid } else { peeled });
                Some(Reference {
                    kind,
                    name: name.to_owned(),
                    id,
                    object_id: text(oid),
                    checked_out: fields.next()? == b"1",
                })
            })
            .collect();
        Ok((refs, upstream, bytes))
    }
}

pub fn short_id(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

pub(crate) fn parse_log(bytes: &[u8]) -> Vec<Commit> {
    let fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    fields
        .chunks_exact(6)
        .filter(|fields| !fields[0].is_empty())
        .map(commit_from_fields)
        .collect()
}

pub(crate) fn commit_from_fields(fields: &[&[u8]]) -> Commit {
    Commit {
        id: text(fields[0]),
        parents: String::from_utf8_lossy(fields[1])
            .split_ascii_whitespace()
            .map(str::to_owned)
            .collect(),
        author: text(fields[2]),
        author_email: text(fields[3]),
        date: text(fields[4]),
        subject: text(fields[5]),
    }
}

/// Shows each stash as a single row connected to the commit it was made on.
/// Git's internal index and untracked-file commits of a stash are hidden.
fn attach_stashes(commits: &mut Vec<Commit>, stashes: &[crate::Stash]) {
    if stashes.is_empty() {
        return;
    }
    let helpers: HashSet<&str> = stashes
        .iter()
        .flat_map(|stash| stash.helpers.iter().map(String::as_str))
        .collect();
    commits.retain(|commit| !helpers.contains(commit.id.as_str()));
    for commit in commits.iter_mut() {
        if let Some(stash) = stashes.iter().find(|stash| stash.id == commit.id) {
            commit.parents = vec![stash.base.clone()];
        }
    }
}
