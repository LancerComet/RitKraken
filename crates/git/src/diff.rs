use std::{
    fs,
    io::{self, Read, Write},
    ops::Range,
    process::{Command, ExitStatus, Stdio},
};

use crate::{
    Change, Error, Repo, Result,
    command::{checked, process_error, read_output, without_optional_locks},
    protocol::text,
    worktree::append_paths,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Scope {
    Worktree,
    Index,
    Commit { id: String, parent: Option<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTarget {
    pub scope: Scope,
    pub change: Change,
}

#[derive(Debug)]
pub struct Patch {
    bytes: Vec<u8>,
    hunks: Vec<Range<usize>>,
    header: usize,
}

pub const PATCH_VIEW_LIMIT: usize = 8 * 1024 * 1024;
const PATCH_OPTIONS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--full-index",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

impl Repo {
    pub fn patch(&self, file: &FileTarget) -> Result<Patch> {
        let (mut git, is_untracked) = self.patch_command(file, "--unified=3");
        let output = read_output(&mut git)?;
        let bytes = if untracked_diff_found_changes(output.status, is_untracked) {
            output.stdout
        } else {
            checked(output)?
        };
        Ok(Patch::new(bytes, file))
    }

    pub fn full_patch(&self, file: &FileTarget) -> Result<Option<Vec<u8>>> {
        let (mut git, is_untracked) = self.patch_command(file, "--unified=1000000");
        read_preview(&mut git, is_untracked)
    }

    fn patch_command(&self, file: &FileTarget, context: &str) -> (Command, bool) {
        let mut git = self.git_with_literal_paths();
        let is_untracked = matches!(file.scope, Scope::Worktree) && file.change.code == b'?';
        match &file.scope {
            Scope::Commit { parent: None, .. } => {
                git.args(["diff-tree", "--root", "--no-commit-id", "-r", "-p", "-M"]);
            }
            _ => {
                git.arg("diff");
            }
        }
        if is_untracked {
            git.arg("--no-index");
        }
        git.args(PATCH_OPTIONS).arg(context);
        match &file.scope {
            Scope::Index => {
                git.arg("--cached");
            }
            Scope::Commit { id, parent } => {
                if let Some(parent) = parent {
                    git.arg("-M").arg(parent);
                }
                git.arg(id);
            }
            Scope::Worktree => {}
        }
        git.arg("--");
        if is_untracked {
            git.arg("/dev/null");
        }
        append_paths(&mut git, &file.change);
        (git, is_untracked)
    }

    pub fn file_contents(&self, file: &FileTarget) -> Result<String> {
        if file.change.code == b'D' {
            return Err("This file was deleted in the selected version.".into());
        }
        let path = &file.change.path;
        let bytes = match &file.scope {
            Scope::Worktree => {
                let path = self.root().join(path);
                let metadata = fs::symlink_metadata(&path).map_err(Error::from)?;
                if metadata.file_type().is_symlink() {
                    return fs::read_link(path)
                        .map(|target| target.to_string_lossy().into_owned())
                        .map_err(Error::from);
                }
                if !metadata.is_file() {
                    return Err("This entry does not have text file contents.".into());
                }
                fs::File::open(path)
                    .and_then(read_preview_bytes)
                    .map_err(Error::from)?
            }
            Scope::Index | Scope::Commit { .. } => {
                let mut object = std::ffi::OsString::new();
                if let Scope::Commit { id, .. } = &file.scope {
                    object.push(id);
                }
                object.push(":");
                object.push(path);
                read_preview(self.git().args(["cat-file", "blob"]).arg(object), false)?
            }
        };
        let bytes = bytes.ok_or("This file exceeds the 8 MiB preview limit.")?;
        if bytes.contains(&0) {
            return Err("Binary file contents cannot be previewed as text.".into());
        }
        String::from_utf8(bytes).map_err(|_| "This file does not contain UTF-8 text.".into())
    }

    pub fn apply_hunk(&self, file: &FileTarget, shown: &Patch, index: usize) -> Result<()> {
        let patch = self.patch(file)?;
        if patch.bytes != shown.bytes {
            return Err(
                "The diff changed on disk. Review the refreshed diff before staging.".into(),
            );
        }
        let range = patch
            .hunks
            .get(index)
            .ok_or("This hunk cannot be staged separately.")?;
        let mut git = self.git();
        git.args(["apply", "--cached", "--whitespace=nowarn"]);
        if file.scope == Scope::Index {
            git.arg("--reverse");
        }
        let mut child = git
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(process_error)?;
        let mut input = child.stdin.take().expect("piped stdin");
        let write_result = input
            .write_all(&patch.bytes[..patch.header])
            .and_then(|_| input.write_all(&patch.bytes[range.clone()]));
        drop(input);
        checked(child.wait_with_output().map_err(process_error)?)?;
        write_result.map_err(process_error)
    }
}

fn untracked_diff_found_changes(status: ExitStatus, is_untracked_diff: bool) -> bool {
    is_untracked_diff && status.code() == Some(1)
}

fn read_preview_bytes(reader: impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(PATCH_VIEW_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() <= PATCH_VIEW_LIMIT).then_some(bytes))
}

fn read_preview(command: &mut Command, is_untracked_diff: bool) -> Result<Option<Vec<u8>>> {
    let mut child = without_optional_locks(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(process_error)?;
    let preview = read_preview_bytes(child.stdout.take().expect("piped stdout"));
    if !matches!(&preview, Ok(Some(_))) {
        let _ = child.kill();
    }
    let status = child.wait().map_err(process_error)?;
    let bytes = preview.map_err(process_error)?;
    if bytes.is_none()
        || status.success()
        || untracked_diff_found_changes(status, is_untracked_diff)
    {
        Ok(bytes)
    } else {
        Err(Error::Git {
            status,
            message: format!("Git could not read this preview ({status})."),
        })
    }
}

impl Patch {
    fn new(bytes: Vec<u8>, file: &FileTarget) -> Self {
        let mut hunks: Vec<Range<usize>> = Vec::new();
        let mut offset = 0;
        let mut headers = 0;
        let mut mode_change = false;
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if line.starts_with(b"diff --git ") {
                headers += 1;
            } else if line.starts_with(b"old mode ") || line.starts_with(b"new mode ") {
                mode_change = true;
            } else if line.starts_with(b"@@ ") {
                if let Some(previous) = hunks.last_mut() {
                    previous.end = offset;
                }
                hunks.push(offset..bytes.len());
            }
            offset += line.len();
        }
        let header = hunks.first().map_or(0, |range| range.start);
        if !matches!(file.scope, Scope::Worktree | Scope::Index)
            || file.change.code != b'M'
            || file.change.previous.is_some()
            || headers != 1
            || mode_change
            || bytes.len() > PATCH_VIEW_LIMIT
        {
            hunks.clear();
        }
        Self {
            bytes,
            hunks,
            header,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn same_file_versions(&self, full_patch: &[u8]) -> bool {
        self.header > 0
            && full_patch
                .strip_prefix(&self.bytes[..self.header])
                .is_some_and(|body| body.starts_with(b"@@ "))
    }

    pub fn first_context_line(&self) -> Option<usize> {
        if self.header == 0 {
            return None;
        }
        let range = self.bytes[self.header..]
            .split(|byte| *byte == b' ')
            .nth(2)?;
        let start = range
            .strip_prefix(b"+")?
            .split(|byte| *byte == b',')
            .next()?;
        std::str::from_utf8(start)
            .ok()?
            .parse::<usize>()
            .ok()
            .filter(|line| *line > 0)
    }

    pub fn hunk_labels(&self) -> Vec<String> {
        self.hunks
            .iter()
            .map(|range| {
                let line = self.bytes[range.clone()]
                    .split(|byte| *byte == b'\n')
                    .next()
                    .unwrap_or_default();
                text(line)
            })
            .collect()
    }
}
