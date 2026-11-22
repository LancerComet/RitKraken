use std::{fs, path::Path};

use super::{FileTarget, Scope, blob_name, read_preview, read_preview_bytes};
use crate::{Error, Repo, Result, RuleError};

/// The byte contents of the two file versions a diff compares.
pub struct FileVersions {
    pub old: Option<Vec<u8>>,
    pub new: Option<Vec<u8>>,
}

impl Repo {
    /// Reads both diff sides without the text checks `file_contents` applies,
    /// so binary files such as images can be compared byte for byte.
    pub fn file_versions(&self, file: &FileTarget) -> Result<FileVersions> {
        Ok(FileVersions {
            old: self.old_version(file)?,
            new: self.new_version(file)?,
        })
    }

    fn old_version(&self, file: &FileTarget) -> Result<Option<Vec<u8>>> {
        if matches!(file.change.code, b'A' | b'?') {
            return Ok(None);
        }
        let path = file.change.previous.as_ref().unwrap_or(&file.change.path);
        match &file.scope {
            Scope::Worktree => self.blob(None, path).map(Some),
            Scope::Index => match self.resolve_ref("HEAD")? {
                Some(_) => self.blob(Some("HEAD"), path).map(Some),
                None => Ok(None),
            },
            Scope::Commit { parent, .. } => match parent {
                Some(parent) => self.blob(Some(parent), path).map(Some),
                None => Ok(None),
            },
        }
    }

    fn new_version(&self, file: &FileTarget) -> Result<Option<Vec<u8>>> {
        if file.change.code == b'D' {
            return Ok(None);
        }
        let path = &file.change.path;
        match &file.scope {
            Scope::Worktree => self.worktree_bytes(path).map(Some),
            Scope::Index => self.blob(None, path).map(Some),
            Scope::Commit { id, .. } => self.blob(Some(id), path).map(Some),
        }
    }

    fn worktree_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        let path = self.root().join(path);
        let metadata = fs::symlink_metadata(&path).map_err(Error::from)?;
        if metadata.file_type().is_symlink() {
            return fs::read_link(path)
                .map(|target| target.to_string_lossy().into_owned().into_bytes())
                .map_err(Error::from);
        }
        if !metadata.is_file() {
            return Err(RuleError::NoFileContents.into());
        }
        fs::File::open(path)
            .and_then(read_preview_bytes)
            .map_err(Error::from)?
            .ok_or_else(|| RuleError::FilePreviewLimit.into())
    }

    fn blob(&self, revision: Option<&str>, path: &Path) -> Result<Vec<u8>> {
        read_preview(
            self.git()
                .args(["cat-file", "blob"])
                .arg(blob_name(revision, path)),
            false,
        )?
        .ok_or_else(|| RuleError::FilePreviewLimit.into())
    }
}
