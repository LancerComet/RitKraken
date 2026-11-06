use std::collections::HashSet;

use crate::{
    Change, Repo, Result,
    protocol::{path_from_bytes, text},
};

use super::{COMMIT_DATE_FORMAT, Commit};

#[derive(Debug, PartialEq, Eq)]
pub struct CommitPerson {
    pub name: String,
    pub email: String,
}

#[derive(Debug)]
pub struct CommitDetail {
    pub message: String,
    pub changes: Vec<Change>,
    pub co_authors: Vec<CommitPerson>,
    pub committer: CommitPerson,
    pub committed_date: String,
}

impl Repo {
    pub fn detail(&self, commit: &Commit) -> Result<CommitDetail> {
        let metadata = self.read(&[
            "show",
            "--no-patch",
            "--no-notes",
            "--no-show-signature",
            "--encoding=UTF-8",
            COMMIT_DATE_FORMAT,
            "--format=%cn%x00%ce%x00%cd%x00%(trailers:key=Co-authored-by,valueonly,unfold)%x00%B",
            &commit.id,
            "--",
        ])?;
        let fields: Vec<_> = metadata.splitn(5, |byte| *byte == 0).collect();
        let [name, email, date, trailers, message] = fields.as_slice() else {
            return Err("Git returned incomplete commit details.".into());
        };
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
        Ok(CommitDetail {
            message: text(message).trim_end().to_owned(),
            changes: parse_name_status(&self.read(&args)?),
            co_authors: co_authors(trailers, &commit.author_email),
            committer: CommitPerson {
                name: text(name),
                email: text(email),
            },
            committed_date: text(date),
        })
    }
}

fn co_authors(trailers: &[u8], author_email: &str) -> Vec<CommitPerson> {
    let mut emails = HashSet::from([author_email.trim().to_lowercase()]);
    text(trailers)
        .lines()
        .filter_map(|value| {
            let (name, email) = value.trim().strip_suffix('>')?.rsplit_once('<')?;
            let (name, email) = (name.trim(), email.trim());
            if name.is_empty()
                || email.is_empty()
                || email.contains(|c: char| c == '>' || c.is_whitespace())
            {
                return None;
            }
            emails.insert(email.to_lowercase()).then(|| CommitPerson {
                name: name.to_owned(),
                email: email.to_owned(),
            })
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
