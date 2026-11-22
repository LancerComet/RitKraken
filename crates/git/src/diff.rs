mod export;
mod hunk;
mod staging;
mod versions;
mod whitespace;

use std::{
    fs,
    io::{self, Read},
    ops::Range,
    path::Path,
    process::{Command, ExitStatus, Stdio},
};

use crate::{
    Change, Error, Repo, Result, RuleError,
    command::{CallTimer, checked, process_error, read_output, without_optional_locks},
    worktree::append_paths,
};
pub use export::ExportedFile;
pub use staging::{PatchSelection, PatchSide};
pub use versions::FileVersions;
pub use whitespace::{align_changed_lines, edge_whitespace_key};

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

#[derive(Debug, Eq, PartialEq)]
pub struct Patch {
    bytes: Vec<u8>,
    hunks: Vec<Range<usize>>,
    header: usize,
    ignore_edge_whitespace: bool,
}

pub const PATCH_VIEW_LIMIT: usize = 8 * 1024 * 1024;
pub(crate) const PATCH_OPTIONS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--full-index",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--output-indicator-old=-",
    "--output-indicator-new=+",
    "--output-indicator-context= ",
    "--submodule=short",
];

impl Repo {
    pub fn patch(&self, file: &FileTarget) -> Result<Patch> {
        self.patch_bytes(file, &[])
            .map(|bytes| Patch::new(bytes, file))
    }

    pub fn export_patch(&self, file: &FileTarget) -> Result<Vec<u8>> {
        self.patch_bytes(file, &["--binary"])
    }

    fn patch_bytes(&self, file: &FileTarget, options: &[&str]) -> Result<Vec<u8>> {
        let (mut git, is_untracked) = self.patch_command(file, "--unified=3", options);
        let output = read_output(&mut git)?;
        if untracked_diff_found_changes(output.status, is_untracked) {
            Ok(output.stdout)
        } else {
            checked(output)
        }
    }

    pub fn full_patch(&self, file: &FileTarget) -> Result<Option<Vec<u8>>> {
        let (mut git, is_untracked) = self.patch_command(file, "--unified=1000000", &[]);
        read_preview(&mut git, is_untracked)
    }

    fn patch_command(&self, file: &FileTarget, context: &str, options: &[&str]) -> (Command, bool) {
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
        git.args(PATCH_OPTIONS).arg(context).args(options);
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
            return Err(RuleError::DeletedFile.into());
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
                    return Err(RuleError::NoFileContents.into());
                }
                fs::File::open(path)
                    .and_then(read_preview_bytes)
                    .map_err(Error::from)?
            }
            Scope::Index | Scope::Commit { .. } => {
                let revision = match &file.scope {
                    Scope::Commit { id, .. } => Some(id.as_str()),
                    _ => None,
                };
                let object = blob_name(revision, path);
                read_preview(self.git().args(["cat-file", "blob"]).arg(object), false)?
            }
        };
        let bytes = bytes.ok_or(RuleError::FilePreviewLimit)?;
        if bytes.contains(&0) {
            return Err(RuleError::BinaryFile.into());
        }
        String::from_utf8(bytes).map_err(|_| RuleError::InvalidFileEncoding.into())
    }
}

fn blob_name(revision: Option<&str>, path: &Path) -> std::ffi::OsString {
    let mut object = std::ffi::OsString::from(revision.unwrap_or_default());
    object.push(":");
    object.push(path);
    object
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

pub(crate) fn read_preview(
    command: &mut Command,
    is_untracked_diff: bool,
) -> Result<Option<Vec<u8>>> {
    let command = without_optional_locks(command);
    let timer = CallTimer::start(command);
    let mut child = match command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(timer) = timer {
                timer.not_started(&error);
            }
            return Err(process_error(error));
        }
    };
    let preview = read_preview_bytes(child.stdout.take().expect("piped stdout"));
    if !matches!(&preview, Ok(Some(_))) {
        let _ = child.kill();
    }
    let status = child.wait().map_err(process_error)?;
    if let Some(timer) = timer {
        timer.finish(Some(status), &[]);
    }
    let bytes = preview.map_err(process_error)?;
    if bytes.is_none()
        || status.success()
        || untracked_diff_found_changes(status, is_untracked_diff)
    {
        Ok(bytes)
    } else {
        Err(Error::PreviewRead(status))
    }
}

impl Patch {
    fn new(bytes: Vec<u8>, file: &FileTarget) -> Self {
        let mut hunks: Vec<Range<usize>> = Vec::new();
        let mut offset = 0;
        let mut headers = 0;
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if line.starts_with(b"diff --git ") {
                headers += 1;
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
            || !staging::is_text_modification(&bytes[..header])
            || bytes.len() > PATCH_VIEW_LIMIT
        {
            hunks.clear();
        }
        Self {
            bytes,
            hunks,
            header,
            ignore_edge_whitespace: false,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn with_edge_whitespace_ignored(mut self, ignore: bool) -> Self {
        // The viewer uses a lossy preview for other encodings. Compare only
        // when the text it shows maps exactly back to the patch bytes.
        self.ignore_edge_whitespace = ignore && std::str::from_utf8(&self.bytes).is_ok();
        self
    }

    pub fn ignores_edge_whitespace(&self) -> bool {
        self.ignore_edge_whitespace
    }

    pub fn same_file_versions(&self, full_patch: &[u8]) -> bool {
        self.header > 0
            && full_patch
                .strip_prefix(&self.bytes[..self.header])
                .is_some_and(|body| body.starts_with(b"@@ "))
    }

    pub fn first_context_line(&self) -> Option<usize> {
        let [_, modified] = hunk::ranges(&self.bytes[self.header..])?;
        (!modified.is_empty()).then_some(modified.start)
    }

    pub fn hunk_count(&self) -> usize {
        self.hunks.len()
    }
}
