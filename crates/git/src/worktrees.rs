mod porcelain;

use std::path::{Path, PathBuf};

use crate::{
    Repo, Result, RuleError,
    command::{command, run_read, run_write},
};

/// A registered worktree, including missing entries that Git can prune.
#[derive(Clone, Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
    pub dirty: bool,
    pub is_current: bool,
    pub is_main: bool,
}

#[derive(Clone, Debug)]
pub struct NewWorktree {
    pub path: PathBuf,
    pub branch: String,
    pub create_branch: bool,
}

impl Repo {
    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        let bytes = self.read(&["worktree", "list", "--porcelain", "-z"])?;
        let mut entries = porcelain::parse(&bytes);
        let root = self.root().canonicalize()?;
        for entry in &mut entries {
            entry.is_current = match entry.path.canonicalize() {
                Ok(path) => registered_path_eq(&path, &root),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
        }
        // Each registered checkout gets its own reader: a worktree's status
        // and identity checks are independent processes.
        std::thread::scope(|scope| -> Result<()> {
            let readers: Vec<_> = entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| !entry.bare && entry.prunable.is_none() && entry.path.is_dir())
                .map(|(index, entry)| {
                    let path = entry.path.clone();
                    let dirty = crate::command::spawn(scope, move || {
                        // Run at the listed root: Repo::open could discover a
                        // parent repository if a missing worktree was replaced
                        // by a directory.
                        let top =
                            run_read(command(Some(&path)).args(["rev-parse", "--show-toplevel"]))?;
                        let top = crate::protocol::path_from_bytes(
                            crate::protocol::without_final_newline(&top),
                        );
                        if !registered_path_eq(&top, &path) {
                            return Err(RuleError::StaleWorktree.into());
                        }
                        Ok(run_read(command(Some(&path)).args([
                            "status",
                            "--porcelain=v1",
                            "-z",
                            "--untracked-files=all",
                            "--ignore-submodules=none",
                        ]))?
                        .is_empty())
                    })?;
                    Ok((index, dirty))
                })
                .collect::<Result<_>>()?;
            for (index, reader) in readers {
                entries[index].dirty = !crate::command::join(reader)?;
            }
            Ok(())
        })?;
        Ok(entries)
    }

    pub fn add_worktree(&self, request: &NewWorktree) -> Result<()> {
        self.validate_ref_name(&request.branch, "heads")?;
        if request.path.as_os_str().is_empty()
            || request.path.as_os_str().as_encoded_bytes().contains(&0)
            || (!request.path.is_absolute()
                && (request.path.has_root()
                    || matches!(
                        request.path.components().next(),
                        Some(std::path::Component::Prefix(_))
                    )))
        {
            return Err(RuleError::InvalidWorktreePath.into());
        }
        // Absolute paths preserve names such as "-topic" literally.
        let path = self.root().join(&request.path);
        let full_branch = format!("refs/heads/{}", request.branch);
        let exists = self.resolve_ref(&full_branch)?.is_some();
        if request.create_branch && exists {
            return Err(RuleError::RefExists.into());
        }
        if !request.create_branch {
            if !exists {
                return Err(RuleError::RefUnavailable.into());
            }
            if self
                .worktrees()?
                .iter()
                .any(|entry| entry.branch.as_deref() == Some(&request.branch))
            {
                return Err(RuleError::BranchInUse.into());
            }
        }
        let mut git = self.git();
        git.args(["worktree", "add", "--no-guess-remote"]);
        if request.create_branch {
            // An explicit start point avoids worktree.guessRemote and always
            // starts from this worktree's current HEAD.
            if self.resolve_ref("HEAD")?.is_none() {
                return Err(RuleError::InvalidCommit.into());
            }
            git.args(["-b", &request.branch, "--"])
                .arg(path)
                .arg("HEAD");
        } else {
            git.arg("--").arg(path).arg(&request.branch);
        }
        run_write(&mut git)
    }

    pub fn remove_worktree(&self, expected: &Worktree) -> Result<()> {
        let entries = self.worktrees()?;
        // Match full registered paths, never Git's suffix lookup. Do not
        // canonicalize this request: an old path could now be a symlink to a
        // different registered worktree.
        let current = entries
            .iter()
            .find(|entry| registered_path_eq(&entry.path, &expected.path))
            .ok_or(RuleError::StaleWorktree)?;
        if current.head != expected.head || current.branch != expected.branch {
            return Err(RuleError::StaleWorktree.into());
        }
        if current.is_main || current.is_current || current.bare {
            return Err(RuleError::ProtectedWorktree.into());
        }
        if current.locked.is_some() {
            return Err(RuleError::LockedWorktree.into());
        }
        if current.dirty {
            return Err(RuleError::DirtyWorktree.into());
        }
        if current.prunable.is_some() || !current.path.is_dir() {
            return Err(RuleError::StaleWorktree.into());
        }
        // Git also checks cleanliness and locks at execution; never force,
        // prune registrations, or delete the associated branch.
        run_write(
            self.git()
                .args(["worktree", "remove", "--"])
                .arg(&current.path),
        )
    }
}

fn registered_path_eq(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        let key = |path: &Path| {
            let path = path.to_string_lossy().replace('\\', "/");
            path.strip_prefix("//?/")
                .unwrap_or(&path)
                .trim_end_matches('/')
                .to_lowercase()
        };
        key(left) == key(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
