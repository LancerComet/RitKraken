mod files;
mod ignore;
mod stash;

use std::{collections::HashSet, path::PathBuf, process::Command};

use crate::{
    Repo, Result, RuleError,
    command::{run_with_input, run_write},
    protocol::path_from_bytes,
};

pub use files::DiscardFile;
pub(crate) use files::WorkspaceVersion;
pub use ignore::IgnoreRule;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub previous: Option<PathBuf>,
    pub code: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Status {
    pub unstaged: Vec<Change>,
    pub staged: Vec<Change>,
}

impl Status {
    pub fn has_conflicts(&self) -> bool {
        self.staged
            .iter()
            .chain(&self.unstaged)
            .any(|change| change.code == b'U')
    }

    pub fn is_empty(&self) -> bool {
        self.unstaged.is_empty() && self.staged.is_empty()
    }

    /// Yields one change per path, preferring its staged classification.
    pub fn distinct_changes(&self) -> impl Iterator<Item = &Change> {
        let mut seen = HashSet::new();
        self.staged
            .iter()
            .chain(&self.unstaged)
            .filter(move |change| seen.insert(change.path.as_path()))
    }
}

impl Repo {
    pub fn status(&self) -> Result<Status> {
        self.read(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
            .map(|bytes| parse_status(&bytes))
    }

    pub fn stage(&self, change: Option<&Change>) -> Result<()> {
        self.stage_files(change.map(std::slice::from_ref))
    }

    pub fn stage_files(&self, changes: Option<&[Change]>) -> Result<()> {
        if changes.is_some_and(<[Change]>::is_empty) {
            return Ok(());
        }
        self.check_conflict_staging(changes)?;
        let mut git = self.git_with_literal_paths();
        git.args(["add", "-A"]);
        run_paths(git, changes)
    }

    pub fn unstage(&self, change: Option<&Change>) -> Result<()> {
        self.unstage_files(change.map(std::slice::from_ref))
    }

    pub fn unstage_files(&self, changes: Option<&[Change]>) -> Result<()> {
        if changes.is_some_and(<[Change]>::is_empty) {
            return Ok(());
        }
        let head = self.resolve_ref("HEAD")?;
        let mut git = self.git_with_literal_paths();
        if head.is_some() {
            git.args(["restore", "--staged"]);
        } else {
            git.args(["rm", "-r", "--cached", "--force"]);
        }
        run_paths(git, changes)
    }

    pub fn commit(&self, summary: &str, description: &str) -> Result<()> {
        let summary = summary.trim();
        if summary.is_empty() {
            return Err(RuleError::EmptyCommitSummary.into());
        }
        let mut git = self.git();
        git.args(["commit", "-m", summary]);
        if !description.trim().is_empty() {
            git.args(["-m", description.trim()]);
        }
        run_write(&mut git)
    }

    pub fn stash(&self) -> Result<()> {
        run_write(self.git().args(["stash", "push", "--include-untracked"]))
    }

    pub fn pop_stash(&self) -> Result<()> {
        if self.pop_file_stash()? {
            return Ok(());
        }
        run_write(self.git().args(["stash", "pop", "--index"]))
    }
}

/// NUL-delimited stdin avoids Windows' command-line limit and preserves literal paths.
fn run_paths(mut git: Command, changes: Option<&[Change]>) -> Result<()> {
    let Some(changes) = changes else {
        return run_write(git.args(["--", "."]));
    };
    let mut paths = Vec::new();
    for change in changes {
        for path in std::iter::once(&change.path)
            .chain(change.previous.as_ref().filter(|_| change.code != b'C'))
        {
            paths.extend_from_slice(path.as_os_str().as_encoded_bytes());
            paths.push(0);
        }
    }
    run_with_input(
        git.args(["--pathspec-from-file=-", "--pathspec-file-nul"]),
        &[&paths],
    )
    .map(drop)
}

pub(crate) fn append_paths(command: &mut Command, change: &Change) {
    command.arg(&change.path);
    if let Some(previous) = &change.previous
        && change.code != b'C'
    {
        command.arg(previous);
    }
}

fn parse_status(bytes: &[u8]) -> Status {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut status = Status::default();
    while let Some(field) = fields.next().filter(|field| field.len() >= 4) {
        let (index, worktree) = (field[0], field[1]);
        let path = path_from_bytes(&field[3..]);
        let rename = matches!(index, b'R' | b'C') || matches!(worktree, b'R' | b'C');
        let previous = if rename {
            fields.next().map(path_from_bytes)
        } else {
            None
        };
        if matches!(
            (index, worktree),
            (b'D', b'D') | (b'A' | b'U', b'U') | (b'U', b'A' | b'D') | (b'D', b'U') | (b'A', b'A')
        ) {
            status.unstaged.push(Change {
                path,
                previous,
                code: b'U',
            });
            continue;
        }
        if !matches!(index, b' ' | b'?' | b'!') {
            status.staged.push(Change {
                path: path.clone(),
                previous: previous.clone(),
                code: index,
            });
        }
        if !matches!(worktree, b' ' | b'!') {
            status.unstaged.push(Change {
                path,
                previous: previous.filter(|_| matches!(worktree, b'R' | b'C')),
                code: worktree,
            });
        }
    }
    status
}
