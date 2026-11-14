mod markers;

pub use markers::has_conflict_markers;

use std::{fs, io::Write};

use crate::{
    DiscardFile, Error, FileTarget, Repo, Result, RuleError, Scope,
    command::{run_read, run_write},
    diff::read_preview,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictVersion {
    pub text: Option<String>,
    pub mode: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictFile {
    pub file: FileTarget,
    pub current: Option<ConflictVersion>,
    pub incoming: Option<ConflictVersion>,
    pub working_text: Option<String>,
    pub unmerged: bool,
    version: DiscardFile,
}

#[derive(Clone, Debug)]
pub enum ConflictResolution {
    Current,
    Incoming,
    Text(String),
    WorkingFile,
}

impl Repo {
    pub fn conflict_file(&self, file: &FileTarget) -> Result<Option<ConflictFile>> {
        if !matches!(file.scope, Scope::Worktree | Scope::Index) {
            return Ok(None);
        }
        if file.change.code != b'U'
            && !self
                .conflict_working_text(file)?
                .as_ref()
                .is_some_and(|text| has_conflict_markers(text.as_bytes()))
        {
            return Ok(None);
        }
        let version = self.prepare_discard(file)?;
        let working_text = self.conflict_working_text(file)?;
        let mut current = None;
        let mut incoming = None;
        let mut unmerged = false;
        for entry in version
            .index()
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let header = entry
                .split(|byte| *byte == b'\t')
                .next()
                .unwrap_or_default();
            let fields: Vec<_> = header.split(|byte| *byte == b' ').collect();
            if fields.len() != 3 {
                return Err(RuleError::StaleFile.into());
            }
            let stage = fields[2];
            unmerged |= stage != b"0";
            if matches!(stage, b"2" | b"3") {
                let value = self.conflict_version(fields[0], fields[1])?;
                if stage == b"2" {
                    current = Some(value);
                } else {
                    incoming = Some(value);
                }
            }
        }
        if !unmerged {
            let bytes = run_read(
                self.git_with_literal_paths()
                    .args(["ls-tree", "-z", "HEAD", "--"])
                    .arg(&file.change.path),
            )?;
            if let Some(entry) = bytes
                .split(|byte| *byte == b'\t')
                .next()
                .filter(|entry| !entry.is_empty())
            {
                let fields: Vec<_> = entry.split(|byte| *byte == b' ').collect();
                if fields.len() == 3 && fields[1] == b"blob" {
                    current = Some(self.conflict_version(fields[0], fields[2])?);
                }
            }
        }
        self.verify_discard(&version)?;
        Ok(Some(ConflictFile {
            file: file.clone(),
            current,
            incoming,
            working_text,
            unmerged,
            version,
        }))
    }

    fn conflict_working_text(&self, file: &FileTarget) -> Result<Option<String>> {
        let mut working_file = file.clone();
        working_file.scope = Scope::Worktree;
        let mut working_text = match self.file_contents(&working_file) {
            Ok(text) => Some(text),
            Err(Error::Rule(
                RuleError::BinaryFile
                | RuleError::InvalidFileEncoding
                | RuleError::FilePreviewLimit
                | RuleError::NoFileContents
                | RuleError::DeletedFile,
            )) => None,
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if fs::symlink_metadata(self.root().join(&file.change.path))
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            working_text = None;
        }
        Ok(working_text)
    }

    fn conflict_version(&self, mode: &[u8], id: &[u8]) -> Result<ConflictVersion> {
        let mode = String::from_utf8_lossy(mode).into_owned();
        let id = String::from_utf8_lossy(id).into_owned();
        let text = if mode.starts_with("100") {
            read_preview(self.git().args(["cat-file", "blob", &id]), false)?
                .filter(|bytes| !bytes.contains(&0))
                .and_then(|bytes| String::from_utf8(bytes).ok())
        } else {
            None
        };
        Ok(ConflictVersion { text, mode, id })
    }

    pub fn resolve_conflict(
        &self,
        expected: &ConflictFile,
        resolution: &ConflictResolution,
    ) -> Result<()> {
        self.verify_discard(&expected.version)?;
        let path = &expected.file.change.path;
        match resolution {
            ConflictResolution::Current | ConflictResolution::Incoming => {
                let current = matches!(resolution, ConflictResolution::Current);
                if !current && !expected.unmerged {
                    return Err(RuleError::StaleFile.into());
                }
                let selected = if current {
                    &expected.current
                } else {
                    &expected.incoming
                };
                if selected.is_none() {
                    if expected.version.index().is_empty() {
                        return run_write(
                            self.git_with_literal_paths()
                                .args(["clean", "--force", "--"])
                                .arg(path),
                        );
                    }
                    return run_write(
                        self.git_with_literal_paths()
                            .args(["rm", "--force", "--"])
                            .arg(path),
                    );
                }
                let mut git = self.git_with_literal_paths();
                git.arg("restore");
                if expected.unmerged {
                    git.arg(if current { "--ours" } else { "--theirs" });
                } else {
                    git.arg("--source=HEAD");
                }
                run_write(git.args(["--worktree", "--"]).arg(path))?;
            }
            ConflictResolution::Text(text) => {
                if has_conflict_markers(text.as_bytes()) {
                    return Err(RuleError::UnresolvedConflicts.into());
                }
                let full = self.root().join(path);
                let parent = full.parent().ok_or(RuleError::InvalidFilePath)?;
                if !parent
                    .canonicalize()?
                    .starts_with(self.root().canonicalize()?)
                {
                    return Err(RuleError::InvalidFilePath.into());
                }
                let metadata = fs::symlink_metadata(&full)?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(RuleError::NoFileContents.into());
                }
                let mut output = tempfile::NamedTempFile::new_in(parent)?;
                output.write_all(text.as_bytes())?;
                output.as_file().set_permissions(metadata.permissions())?;
                output.persist(full).map_err(|error| error.error)?;
            }
            ConflictResolution::WorkingFile => {
                if self.worktree_has_conflict_markers(path)? {
                    return Err(RuleError::UnresolvedConflicts.into());
                }
            }
        }
        run_write(
            self.git_with_literal_paths()
                .args(["add", "-A", "--"])
                .arg(path),
        )
    }

    pub(crate) fn check_conflict_staging(&self, change: Option<&crate::Change>) -> Result<()> {
        for conflicted in self
            .status()?
            .unstaged
            .into_iter()
            .filter(|entry| entry.code == b'U')
        {
            if change.is_some_and(|selected| selected.path != conflicted.path) {
                continue;
            }
            let file = FileTarget {
                scope: Scope::Worktree,
                change: conflicted,
            };
            match self.file_contents(&file) {
                Ok(text) if !has_conflict_markers(text.as_bytes()) => {}
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(RuleError::UnresolvedConflicts.into()),
            }
        }
        Ok(())
    }

    fn worktree_has_conflict_markers(&self, path: &std::path::Path) -> Result<bool> {
        let path = self.root().join(path);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Ok(false),
            Ok(metadata) if metadata.is_file() => Ok(markers::contains_markers(
                std::io::BufReader::new(fs::File::open(path)?),
            )?),
            Ok(_) => Err(RuleError::NoFileContents.into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn check_resolved_markers(&self) -> Result<()> {
        for change in self.status()?.distinct_changes() {
            if self.worktree_has_conflict_markers(&change.path)? {
                return Err(RuleError::UnresolvedConflicts.into());
            }
        }
        Ok(())
    }
}
