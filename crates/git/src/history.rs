use crate::{
    Change, Repo, Result, Status, Upstream,
    protocol::{path_from_bytes, text},
};

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct Reference {
    pub kind: RefKind,
    pub name: String,
    pub id: String,
}

#[derive(Debug)]
pub struct Snapshot {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub status: Status,
    pub refs: Vec<Reference>,
    pub remotes: Vec<String>,
    pub upstream: Option<Upstream>,
    pub has_stash: bool,
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
}

#[derive(Debug)]
pub struct CommitDetail {
    pub message: String,
    pub changes: Vec<Change>,
}

impl Repo {
    pub fn snapshot(&self, history_limit: Option<usize>) -> Result<Snapshot> {
        let head = self.resolve_ref("HEAD")?;
        let branch = self.checked_out_branch()?;
        let status = self.status()?;
        let (refs, upstream) = self.references(branch.as_deref())?;
        let remotes = self.remotes()?;
        let has_stash = self.resolve_ref("refs/stash")?.is_some();
        let mut args = vec![
            "log",
            "--exclude=refs/stash",
            "--all",
            "--topo-order",
            "--date=format-local:%Y/%m/%d @ %H:%M",
            "--encoding=UTF-8",
            "--format=%H%x00%P%x00%an%x00%ae%x00%ad%x00%s",
            "-z",
        ];
        let limit = history_limit.map(|limit| format!("--max-count={}", limit.saturating_add(1)));
        if let Some(limit) = limit.as_deref() {
            args.push(limit);
        }
        let mut commits = parse_log(&self.read(&args)?);
        let history_limited = history_limit.is_some_and(|limit| commits.len() > limit);
        if let Some(limit) = history_limit {
            commits.truncate(limit);
        }
        Ok(Snapshot {
            branch,
            head,
            status,
            refs,
            remotes,
            upstream,
            has_stash,
            commits,
            history_limited,
        })
    }

    pub fn detail(&self, commit: &Commit) -> Result<CommitDetail> {
        let message = self.read(&["show", "--no-patch", "--format=%B", &commit.id, "--"])?;
        let args = match commit.parents.first() {
            Some(parent) => vec![
                "diff",
                "--name-status",
                "-z",
                "-M",
                parent,
                &commit.id,
                "--",
            ],
            None => vec![
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-status",
                "-r",
                "-z",
                "-M",
                &commit.id,
                "--",
            ],
        };
        let changes = parse_name_status(&self.read(&args)?);
        Ok(CommitDetail {
            message: text(&message).trim_end().to_owned(),
            changes,
        })
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

    pub(crate) fn references(
        &self,
        branch: Option<&str>,
    ) -> Result<(Vec<Reference>, Option<Upstream>)> {
        let bytes = self.read(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(symref)%00%(upstream:remotename)%00%(upstream:remoteref)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
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
                if kind == RefKind::Local && Some(name) == branch {
                    let remote = fields.next()?;
                    let reference = fields.next()?;
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
                })
            })
            .collect();
        Ok((refs, upstream))
    }
}

pub fn short_id(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

fn parse_log(bytes: &[u8]) -> Vec<Commit> {
    let fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    fields
        .chunks_exact(6)
        .filter(|fields| !fields[0].is_empty())
        .map(|fields| Commit {
            id: text(fields[0]),
            parents: String::from_utf8_lossy(fields[1])
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect(),
            author: text(fields[2]),
            author_email: text(fields[3]),
            date: text(fields[4]),
            subject: text(fields[5]),
        })
        .collect()
}

fn parse_name_status(bytes: &[u8]) -> Vec<Change> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut changes = Vec::new();
    while let Some(code) = fields.next().and_then(|field| field.first()).copied() {
        let Some(first) = fields.next() else { break };
        let (path, previous) = if matches!(code, b'R' | b'C') {
            let Some(second) = fields.next() else { break };
            (path_from_bytes(second), Some(path_from_bytes(first)))
        } else {
            (path_from_bytes(first), None)
        };
        changes.push(Change {
            path,
            previous,
            code,
        });
    }
    changes
}
