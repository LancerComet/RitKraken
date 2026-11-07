use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Repo, Result, RuleError, command::run_read};

use super::blob_name;

/// A read-only Git blob whose temporary directory lives as long as its owner.
#[derive(Debug)]
pub struct ExportedFile {
    path: PathBuf,
    permissions: fs::Permissions,
    _directory: tempfile::TempDir,
}

impl ExportedFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ExportedFile {
    fn drop(&mut self) {
        // Restore the temporary file's original permissions before TempDir
        // removes it, including Windows' read-only attribute.
        let _ = fs::set_permissions(&self.path, self.permissions.clone());
    }
}

impl Repo {
    pub fn export_commit_file(&self, revision: &str, source: &Path) -> Result<ExportedFile> {
        let filename = source.file_name().ok_or(RuleError::NoFileContents)?;
        let directory = tempfile::Builder::new()
            .prefix("ritkraken-file-")
            .tempdir()?;
        let path = directory.path().join(filename);
        let output = fs::File::create(&path)?;
        let mut command = self.git();
        command
            .args(["cat-file", "blob"])
            .arg(blob_name(Some(revision), source))
            .stdout(output);
        run_read(&mut command)?;
        drop(command);
        let permissions = fs::metadata(&path)?.permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only)?;
        Ok(ExportedFile {
            path,
            permissions,
            _directory: directory,
        })
    }
}
