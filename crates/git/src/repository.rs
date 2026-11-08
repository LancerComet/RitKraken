use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    Result,
    command::{checked, command, read_output, run_read, run_write},
    protocol::{path_from_bytes, text, without_final_newline},
};

#[derive(Clone, Debug)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    pub fn open(path: &Path) -> Result<Self> {
        let root = run_read(command(Some(path)).args(["rev-parse", "--show-toplevel"]))?;
        Ok(Self {
            root: path_from_bytes(without_final_newline(&root)),
        })
    }

    pub fn name(&self) -> String {
        Self::name_from_path(&self.root)
    }

    pub fn name_from_path(path: &Path) -> String {
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }

    pub fn init(path: &Path, branch: Option<&str>) -> Result<Self> {
        let mut git = command(None);
        git.arg("init");
        if let Some(branch) = branch {
            git.arg(format!("--initial-branch={branch}"));
        }
        run_write(git.arg("--").arg(path))?;
        Self::open(path)
    }

    pub fn clone(url: &str, path: &Path) -> Result<Self> {
        run_write(command(None).args(["clone", "--", url]).arg(path))?;
        Self::open(path)
    }

    pub fn switch(&self, name: &str) -> Result<()> {
        run_write(self.git().args(["switch", "--no-guess", "--", name]))
    }

    pub fn create_branch(&self, name: &str) -> Result<()> {
        self.create_branch_at(name, None, true, None)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn git(&self) -> Command {
        command(Some(&self.root))
    }

    pub(crate) fn git_with_literal_paths(&self) -> Command {
        let mut git = self.git();
        git.env("GIT_LITERAL_PATHSPECS", "1");
        git
    }

    pub(crate) fn read(&self, args: &[&str]) -> Result<Vec<u8>> {
        run_read(self.git().args(args))
    }

    fn optional(&self, args: &[&str]) -> Result<Option<String>> {
        let output = read_output(self.git().args(args))?;
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        checked(output).map(|bytes| Some(text(without_final_newline(&bytes))))
    }

    pub(crate) fn resolve_ref(&self, reference: &str) -> Result<Option<String>> {
        self.optional(&["rev-parse", "--verify", "--quiet", reference])
    }

    pub(crate) fn checked_out_branch(&self) -> Result<Option<String>> {
        self.optional(&["symbolic-ref", "--quiet", "HEAD"])
            .map(|reference| {
                reference
                    .and_then(|reference| reference.strip_prefix("refs/heads/").map(str::to_owned))
            })
    }
}
