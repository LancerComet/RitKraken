use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{
    Change, FileTarget, Repo, Result, RuleError, Scope,
    command::{join, run_read, run_with_input, run_write, spawn},
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
        let (status, index) = std::thread::scope(|scope| -> Result<_> {
            let status = spawn(scope, || self.status())?;
            let index = spawn(scope, || self.read(&["ls-files", "--stage", "-z"]))?;
            Ok((join(status)?, join(index)?))
        })?;
        let mut paths: Vec<_> = status
            .distinct_changes()
            .flat_map(|change| std::iter::once(change.path.clone()).chain(change.previous.clone()))
            .collect();
        paths.sort();
        paths.dedup();
        // Non-recursive Git operations leave submodule working trees alone.
        paths.retain(|path| !self.root().join(path).is_dir());
        let worktree = self.worktree_versions(&paths)?;
        Ok(WorkspaceVersion {
            status,
            index,
            worktree,
        })
    }

    pub fn prepare_discard(&self, file: &FileTarget) -> Result<DiscardFile> {
        let prepared = self.prepare_discards(std::slice::from_ref(file))?;
        let Some(prepared) = prepared.into_iter().next() else {
            return Err(RuleError::StaleFile.into());
        };
        Ok(prepared)
    }

    /// Reads the versions for a whole selection with one process per Git
    /// command instead of one per file.
    pub fn prepare_discards(&self, files: &[FileTarget]) -> Result<Vec<DiscardFile>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }
        for file in files {
            validate_change(&file.change)?;
        }
        let mut index_git = self.git_with_literal_paths();
        index_git.args(["ls-files", "--stage", "-z", "--"]);
        for file in files {
            append_paths(&mut index_git, &file.change);
        }
        let (status, index, head) = std::thread::scope(|scope| -> Result<_> {
            let status = spawn(scope, || self.status())?;
            let index = spawn(scope, || run_read(&mut index_git))?;
            let head = spawn(scope, || self.resolve_ref("HEAD"))?;
            Ok((join(status)?, join(index)?, join(head)?))
        })?;
        let mut paths: Vec<_> = files
            .iter()
            .flat_map(|file| {
                let change = &file.change;
                // The rename's old name is hashed only for a real rename.
                std::iter::once(change.path.clone())
                    .chain(change.previous.clone().filter(|_| change.code == b'R'))
            })
            .collect();
        paths.sort();
        paths.dedup();
        // Non-recursive Git operations leave submodule working trees alone.
        paths.retain(|path| !self.root().join(path).is_dir());
        let worktree = self.worktree_versions(&paths)?;
        let versions: HashMap<_, _> = worktree.into_iter().collect();
        let index_records: Vec<&[u8]> = index
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
            .collect();
        for file in files {
            let mut owned_paths: HashSet<&Path> =
                std::iter::once(&*file.change.path).collect();
            if let Some(previous) = &file.change.previous
                && file.change.code != b'C'
            {
                owned_paths.insert(previous);
            }
            let gitlink = index_records.iter().any(|record| {
                let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
                    return false;
                };
                record[..tab].starts_with(b"160000")
                    && owned_paths.contains(
                        crate::protocol::path_from_bytes(&record[tab + 1..]).as_path(),
                    )
            });
            if gitlink || self.root().join(&file.change.path).is_dir() {
                return Err(RuleError::SubmoduleDiscard.into());
            }
        }
        files
            .iter()
            .map(|file| {
                let changes = match file.scope {
                    Scope::Worktree => &status.unstaged,
                    Scope::Index => &status.staged,
                    Scope::Commit { .. } => return Err(RuleError::ReadOnlyFile.into()),
                };
                if !changes.contains(&file.change) {
                    return Err(RuleError::StaleFile.into());
                }
                let mut owned_paths: HashSet<&Path> = std::iter::once(&*file.change.path).collect();
                if let Some(previous) = &file.change.previous
                    && file.change.code != b'C'
                {
                    owned_paths.insert(previous);
                }
                let mut index = Vec::new();
                for record in &index_records {
                    let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
                        continue;
                    };
                    let path = crate::protocol::path_from_bytes(&record[tab + 1..]);
                    if owned_paths.contains(path.as_path()) {
                        index.extend_from_slice(record);
                        index.push(0);
                    }
                }
                let mut worktree = vec![
                    versions
                        .get(&file.change.path)
                        .cloned()
                        .ok_or(RuleError::NoFileContents)?,
                ];
                if let Some(previous) = &file.change.previous
                    && file.change.code == b'R'
                {
                    let old = versions
                        .get(previous)
                        .cloned()
                        .ok_or(RuleError::NoFileContents)?;
                    // A newly created file at the old name is outside this rename.
                    if old != WorktreeVersion::Missing {
                        return Err(RuleError::StaleFile.into());
                    }
                    worktree.push(old);
                }
                Ok(DiscardFile {
                    file: file.clone(),
                    head: head.clone(),
                    index,
                    worktree,
                })
            })
            .collect()
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

    pub fn discard_files(&self, expected: &[DiscardFile]) -> Result<()> {
        // Validate the complete selection before discarding any selected file.
        let files: Vec<_> = expected.iter().map(|file| file.file.clone()).collect();
        if self.prepare_discards(&files)?.as_slice() != expected {
            return Err(RuleError::StaleFile.into());
        }
        for file in expected {
            self.discard_file(file)?;
        }
        Ok(())
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

    /// Classifies each path and hashes every regular file in one
    /// `hash-object --stdin-paths` process instead of a process per file.
    fn worktree_versions(&self, paths: &[PathBuf]) -> Result<Vec<(PathBuf, WorktreeVersion)>> {
        let mut versions: Vec<Option<WorktreeVersion>> = Vec::with_capacity(paths.len());
        let mut files = Vec::new();
        for path in paths {
            let full_path = self.root().join(path);
            match fs::symlink_metadata(&full_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    versions.push(Some(WorktreeVersion::Symlink(fs::read_link(full_path)?)));
                }
                Ok(metadata) if metadata.is_file() => {
                    files.push(path.clone());
                    versions.push(None);
                }
                Ok(_) => return Err(RuleError::NoFileContents.into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    versions.push(Some(WorktreeVersion::Missing));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let mut hashed = self.hash_objects(&files)?.into_iter();
        Ok(paths
            .iter()
            .zip(versions)
            .map(|(path, version)| {
                (
                    path.clone(),
                    version.unwrap_or_else(|| {
                        WorktreeVersion::File(hashed.next().expect("hashed file"))
                    }),
                )
            })
            .collect())
    }

    fn hash_objects(&self, paths: &[PathBuf]) -> Result<Vec<Vec<u8>>> {
        let mut input = Vec::new();
        for path in paths {
            let encoded = path.as_os_str().as_encoded_bytes();
            if encoded.contains(&b'\n') {
                // stdin paths are line-separated; hash the unusual name alone.
                return paths.iter().map(|path| self.hash_object(path)).collect();
            }
            input.extend_from_slice(encoded);
            input.push(b'\n');
        }
        if input.is_empty() {
            return Ok(Vec::new());
        }
        let mut git = self.git();
        git.args(["hash-object", "--no-filters", "--stdin-paths"]);
        let bytes = run_with_input(&mut git, &[&input])?;
        let hashed: Vec<Vec<u8>> = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut oid = line.to_vec();
                oid.push(b'\n');
                oid
            })
            .collect();
        if hashed.len() != paths.len() {
            return Err(RuleError::StaleFile.into());
        }
        Ok(hashed)
    }

    fn hash_object(&self, path: &Path) -> Result<Vec<u8>> {
        run_read(
            self.git()
                .args(["hash-object", "--no-filters", "--"])
                .arg(path),
        )
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
