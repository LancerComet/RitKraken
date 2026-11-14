use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{
    Change, FileTarget, Repo, Result, RuleError, Scope,
    command::{run_read, run_write},
    worktree::append_paths,
};

/// The versions the user reviewed before confirming a discard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscardFile {
    file: FileTarget,
    head: Option<String>,
    index: Vec<u8>,
    worktree: Vec<WorktreeVersion>,
}

impl DiscardFile {
    pub(crate) fn index(&self) -> &[u8] {
        &self.index
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WorktreeVersion {
    Missing,
    File(Vec<u8>),
    Symlink(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceVersion {
    status: crate::Status,
    index: Vec<u8>,
    worktree: Vec<(PathBuf, WorktreeVersion)>,
}

impl Repo {
    pub(crate) fn workspace_version(&self) -> Result<WorkspaceVersion> {
        let status = self.status()?;
        let index = self.read(&["ls-files", "--stage", "-z"])?;
        let mut paths: Vec<_> = status
            .distinct_changes()
            .flat_map(|change| std::iter::once(change.path.clone()).chain(change.previous.clone()))
            .collect();
        paths.sort();
        paths.dedup();
        let mut worktree = Vec::new();
        for path in paths {
            // Non-recursive Git operations leave submodule working trees alone.
            if self.root().join(&path).is_dir() {
                continue;
            }
            worktree.push((path.clone(), self.worktree_version(&path)?));
        }
        Ok(WorkspaceVersion {
            status,
            index,
            worktree,
        })
    }

    pub fn prepare_discard(&self, file: &FileTarget) -> Result<DiscardFile> {
        validate_change(&file.change)?;
        let status = self.status()?;
        let changes = match file.scope {
            Scope::Worktree => &status.unstaged,
            Scope::Index => &status.staged,
            Scope::Commit { .. } => return Err(RuleError::ReadOnlyFile.into()),
        };
        if !changes.contains(&file.change) {
            return Err(RuleError::StaleFile.into());
        }
        let mut git = self.git_with_literal_paths();
        git.args(["ls-files", "--stage", "-z", "--"]);
        append_paths(&mut git, &file.change);
        let index = run_read(&mut git)?;
        let mut worktree = vec![self.worktree_version(&file.change.path)?];
        if let Some(previous) = &file.change.previous
            && file.change.code == b'R'
        {
            let old = self.worktree_version(previous)?;
            // A newly created file at the old name is outside this rename.
            if old != WorktreeVersion::Missing {
                return Err(RuleError::StaleFile.into());
            }
            worktree.push(old);
        }
        Ok(DiscardFile {
            file: file.clone(),
            head: self.resolve_ref("HEAD")?,
            index,
            worktree,
        })
    }

    pub fn discard_file(&self, expected: &DiscardFile) -> Result<()> {
        self.verify_discard(expected)?;
        let mut git = self.git_with_literal_paths();
        match expected.file.scope {
            Scope::Worktree if expected.file.change.code == b'?' => {
                git.args(["clean", "--force", "--"]);
            }
            Scope::Worktree => {
                git.args(["restore", "--worktree", "--"]);
            }
            Scope::Index if expected.head.is_none() => {
                git.args(["rm", "--force", "--"]);
            }
            Scope::Index => {
                git.args(["restore", "--source=HEAD", "--staged", "--worktree", "--"]);
            }
            Scope::Commit { .. } => return Err(RuleError::ReadOnlyFile.into()),
        }
        append_paths(&mut git, &expected.file.change);
        run_write(&mut git)
    }

    pub(crate) fn verify_discard(&self, expected: &DiscardFile) -> Result<()> {
        if self.prepare_discard(&expected.file)? != *expected {
            return Err(RuleError::StaleFile.into());
        }
        Ok(())
    }

    pub fn stop_tracking(&self, change: &Change) -> Result<()> {
        validate_change(change)?;
        let mut git = self.git_with_literal_paths();
        git.args(["rm", "--cached", "--"]);
        git.arg(&change.path);
        run_write(&mut git)
    }

    fn worktree_version(&self, path: &Path) -> Result<WorktreeVersion> {
        let full_path = self.root().join(path);
        match fs::symlink_metadata(&full_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Ok(WorktreeVersion::Symlink(fs::read_link(full_path)?))
            }
            Ok(metadata) if metadata.is_file() => run_read(
                self.git()
                    .args(["hash-object", "--no-filters", "--"])
                    .arg(path),
            )
            .map(WorktreeVersion::File),
            Ok(_) => Err(RuleError::NoFileContents.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(WorktreeVersion::Missing),
            Err(error) => Err(error.into()),
        }
    }
}

pub(super) fn validate_change(change: &Change) -> Result<()> {
    validate_path(&change.path)?;
    if let Some(previous) = &change.previous {
        validate_path(previous)?;
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path.components().any(|part| {
            part.as_os_str()
                .to_str()
                .is_some_and(|part| part.eq_ignore_ascii_case(".git"))
        })
    {
        return Err(RuleError::InvalidFilePath.into());
    }
    Ok(())
}
