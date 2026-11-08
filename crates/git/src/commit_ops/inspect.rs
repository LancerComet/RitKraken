use std::{path::Path, process::Command};

use crate::{
    Repo, Result, RuleError,
    command::run_read,
    diff::{PATCH_OPTIONS, read_preview},
};

impl Repo {
    pub fn compare_commits(&self, from: &str, to: &str) -> Result<Vec<u8>> {
        self.commit_at(from)?;
        self.commit_at(to)?;
        let mut git = self.git();
        git.arg("diff")
            .args(PATCH_OPTIONS)
            .args(["-M", from, to, "--"]);
        read_preview(&mut git, false)?.ok_or_else(|| RuleError::FilePreviewLimit.into())
    }

    pub fn export_commit_patch(
        &self,
        id: &str,
        mainline: Option<usize>,
        path: &Path,
    ) -> Result<()> {
        let mut command = self.commit_patch_command(id, mainline)?;
        let parent = path.parent().ok_or(RuleError::InvalidFilePath)?;
        let pending = tempfile::NamedTempFile::new_in(parent)?;
        command.stdout(pending.reopen()?);
        run_read(&mut command)?;
        drop(command);
        pending.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    fn commit_patch_command(&self, id: &str, mainline: Option<usize>) -> Result<Command> {
        let commit = self.commit_at(id)?;
        let mut git = self.git();
        if commit.parents.len() > 1 {
            let parent = mainline
                .and_then(|number| number.checked_sub(1))
                .and_then(|index| commit.parents.get(index))
                .ok_or(RuleError::MainlineRequired)?;
            git.arg("diff")
                .args(PATCH_OPTIONS)
                .args(["--binary", "-M", parent, id, "--"]);
        } else {
            if mainline.is_some() {
                return Err(RuleError::MainlineRequired.into());
            }
            git.args([
                "format-patch",
                "--stdout",
                "--binary",
                "--full-index",
                "--no-signature",
                "--root",
                "-1",
                id,
                "--",
            ]);
        }
        Ok(git)
    }
}
