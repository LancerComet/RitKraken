use std::path::PathBuf;

use crate::{
    Change, FileTarget, Repo, Result, RuleError, Scope,
    command::{run_read, run_with_input},
    protocol::{path_from_bytes, text},
};

use super::{COMMIT_DATE_FORMAT, COMMIT_LOG_FORMAT, Commit, commit_from_fields};

#[derive(Clone, Debug)]
pub struct FileCommit {
    pub commit: Commit,
    pub change: Change,
}

#[derive(Debug)]
pub struct FileHistory {
    pub commits: Vec<FileCommit>,
    pub has_more: bool,
}

#[derive(Debug)]
pub struct BlameLine {
    pub commit: String,
    pub author: String,
    pub email: String,
    pub number: usize,
    pub text: String,
}

impl Repo {
    pub fn file_history(&self, change: &Change, limit: usize) -> Result<FileHistory> {
        self.file_history_at(
            &FileTarget {
                scope: Scope::Worktree,
                change: change.clone(),
            },
            limit,
        )
    }

    pub fn file_history_at(&self, file: &FileTarget, limit: usize) -> Result<FileHistory> {
        let (revision, path) = match &file.scope {
            Scope::Commit { id, .. } => (id.as_str(), file.change.path.clone()),
            Scope::Worktree | Scope::Index => {
                if self.resolve_ref("HEAD")?.is_none() {
                    return Ok(FileHistory {
                        commits: Vec::new(),
                        has_more: false,
                    });
                }
                ("HEAD", self.file_history_path(&file.change)?)
            }
        };
        let bytes = run_read(
            self.git_with_literal_paths()
                .args([
                    "log",
                    "--follow",
                    "--name-status",
                    "-z",
                    "--no-show-signature",
                    COMMIT_DATE_FORMAT,
                    COMMIT_LOG_FORMAT,
                    "--encoding=UTF-8",
                ])
                .arg(format!("--max-count={}", limit.saturating_add(1)))
                .args([revision, "--"])
                .arg(&path),
        )?;
        let mut commits = parse_file_log(&bytes, path);
        let has_more = commits.len() > limit;
        commits.truncate(limit);
        Ok(FileHistory { commits, has_more })
    }

    pub fn blame_file(&self, file: &FileTarget) -> Result<Vec<BlameLine>> {
        let contents = self.file_contents(file)?;
        if contents.lines().count() > 100_000 {
            return Err(RuleError::BlamePreviewLimit.into());
        }
        let mut command = self.git_with_literal_paths();
        command.args(["blame", "--line-porcelain", "--encoding=UTF-8"]);
        let bytes = match &file.scope {
            Scope::Commit { id, .. } => run_read(command.arg(id).arg("--").arg(&file.change.path))?,
            Scope::Worktree | Scope::Index => {
                let path = self.file_history_path(&file.change)?;
                run_with_input(
                    command.args(["--contents", "-", "--"]).arg(path),
                    &[contents.as_bytes()],
                )?
            }
        };
        Ok(parse_blame(&bytes))
    }

    fn file_history_path(&self, change: &Change) -> Result<PathBuf> {
        if let Some(previous) = &change.previous {
            return Ok(previous.clone());
        }
        let staged = self.status()?.staged;
        let previous = staged
            .iter()
            .find(|staged| staged.path == change.path)
            .and_then(|staged| staged.previous.as_ref());
        Ok(previous.unwrap_or(&change.path).clone())
    }
}

fn parse_file_log(bytes: &[u8], mut path: PathBuf) -> Vec<FileCommit> {
    let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    let mut cursor = 0;
    let mut commits = Vec::new();
    while cursor < fields.len() {
        if fields[cursor].is_empty() {
            cursor += 1;
            continue;
        }
        let Some(metadata) = fields.get(cursor..cursor + 6) else {
            break;
        };
        let commit = commit_from_fields(metadata);
        cursor += 6;
        let mut change = Change {
            path: path.clone(),
            previous: None,
            code: b'M',
        };
        while let Some(status) = fields.get(cursor) {
            let status = status.trim_ascii_start();
            let Some(&code) = status.first() else {
                cursor += 1;
                continue;
            };
            if status.len() > 4
                || !matches!(code, b'A' | b'M' | b'D' | b'R' | b'C' | b'T' | b'U')
                || !status[1..].iter().all(u8::is_ascii_digit)
            {
                break;
            }
            let Some(first) = fields.get(cursor + 1) else {
                break;
            };
            cursor += 2;
            let (new_path, previous) = if matches!(code, b'R' | b'C') {
                let Some(second) = fields.get(cursor) else {
                    break;
                };
                cursor += 1;
                (path_from_bytes(second), Some(path_from_bytes(first)))
            } else {
                (path_from_bytes(first), None)
            };
            change = Change {
                path: new_path,
                previous,
                code,
            };
        }
        path = change.previous.as_ref().unwrap_or(&change.path).clone();
        commits.push(FileCommit { commit, change });
    }
    commits
}

fn parse_blame(bytes: &[u8]) -> Vec<BlameLine> {
    let mut result = Vec::new();
    let (mut commit, mut author, mut email) = (String::new(), String::new(), String::new());
    let mut number = 0;
    for line in bytes.split(|byte| *byte == b'\n') {
        if let Some(content) = line.strip_prefix(b"\t") {
            result.push(BlameLine {
                commit: commit.clone(),
                author: author.clone(),
                email: email.clone(),
                number,
                text: text(content.strip_suffix(b"\r").unwrap_or(content)),
            });
        } else if let Some(value) = line.strip_prefix(b"author ") {
            author = text(value);
        } else if let Some(value) = line.strip_prefix(b"author-mail ") {
            email = text(value).trim_matches(['<', '>']).to_owned();
        } else {
            let fields: Vec<_> = line.split(|byte| *byte == b' ').collect();
            if fields.len() >= 3
                && matches!(fields[0].len(), 40 | 64)
                && fields[0].iter().all(u8::is_ascii_hexdigit)
            {
                commit = text(fields[0]);
                number = text(fields[2]).parse().unwrap_or(0);
            }
        }
    }
    result
}
