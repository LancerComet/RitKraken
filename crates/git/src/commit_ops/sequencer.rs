use std::fs;

use crate::{
    Repo, Result,
    command::{checked, read_output, run_write},
    protocol::text,
};

impl Repo {
    /// Complete a resolved single --no-commit pick while preserving its staged changes.
    pub(super) fn finish_applied_cherry_pick(&self) -> Result<bool> {
        let directory = self.git_path("sequencer")?;
        let options = directory.join("opts");
        if !options.is_file() {
            return Ok(false);
        }
        let output = read_output(self.git().args(["config", "--file"]).arg(options).args([
            "--bool",
            "--get",
            "options.no-commit",
        ]))?;
        if output.status.code() == Some(1) {
            return Ok(false);
        }
        if text(&checked(output)?).trim() != "true" {
            return Ok(false);
        }
        let todo = fs::read_to_string(directory.join("todo"))?;
        let mut commands = todo
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'));
        // A caller continuing an external multi-commit sequence still belongs to Git.
        if !commands
            .next()
            .is_some_and(|line| line.starts_with("pick "))
            || commands.next().is_some()
        {
            return Ok(false);
        }
        // --continue would try to replay this no-commit pick over the resolved
        // index. --quit clears only the sequence, leaving HEAD and the index intact.
        self.check_resolved_markers()?;
        run_write(self.git().args(["cherry-pick", "--quit"]))?;
        Ok(true)
    }
}
