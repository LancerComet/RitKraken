use std::{collections::HashSet, ffi::OsString, fs, path::Path, process::Command};

use super::{CommitAction, CommitAvailability, CommitContext};
use crate::{
    Commit, Repo, Result, RuleError,
    command::run_write,
    history::{COMMIT_DATE_FORMAT, COMMIT_LOG_FORMAT, commit_from_fields},
    protocol::text,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl RebaseAction {
    pub const ALL: [Self; 6] = [
        Self::Pick,
        Self::Reword,
        Self::Edit,
        Self::Squash,
        Self::Fixup,
        Self::Drop,
    ];
    pub fn command(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reword => "reword",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RebaseEntry {
    pub commit: Commit,
    pub action: RebaseAction,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct RebasePlan {
    pub base: String,
    pub entries: Vec<RebaseEntry>,
}

/// Git invokes the application's small editor mode without launching the GUI.
#[derive(Clone, Debug)]
pub struct GitEditor {
    pub sequence: String,
    pub message: String,
}

impl GitEditor {
    pub fn for_executable(path: &Path) -> Self {
        let quoted = shell_quote(&path.to_string_lossy());
        Self {
            sequence: format!("{quoted} --ritkraken-git-editor todo"),
            message: format!("{quoted} --ritkraken-git-editor message"),
        }
    }

    pub(crate) fn configure(&self, repo: &Repo, command: &mut Command) -> Result<()> {
        command
            .env("GIT_SEQUENCE_EDITOR", &self.sequence)
            .env("GIT_EDITOR", &self.message)
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("RITKRAKEN_REBASE_DATA", repo.git_path("ritkraken-rebase")?)
            .env("RITKRAKEN_REBASE_DONE", repo.git_path("rebase-merge/done")?);
        Ok(())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Called before GPUI initialization. Returns None for normal application launches.
pub fn run_git_editor() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--ritkraken-git-editor")) {
        return None;
    }
    Some(editor(args.collect()))
}

fn editor(args: Vec<OsString>) -> Result<()> {
    let [mode, destination] = args.as_slice() else {
        return Err(RuleError::InvalidRebasePlan.into());
    };
    let directory = std::env::var_os("RITKRAKEN_REBASE_DATA")
        .map(std::path::PathBuf::from)
        .ok_or(RuleError::InvalidRebasePlan)?;
    if mode == "todo" {
        fs::copy(directory.join("todo"), destination)?;
    } else if mode == "message" {
        if let Some(done) = std::env::var_os("RITKRAKEN_REBASE_DONE")
            && let Ok(contents) = fs::read_to_string(done)
            && let Some(line) = contents
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty() && !line.starts_with('#'))
        {
            let mut fields = line.split_ascii_whitespace();
            if fields.next() == Some("reword")
                && let Some(id) = fields.next()
                && matches!(id.len(), 40 | 64)
                && id.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                fs::copy(directory.join(format!("{id}.message")), destination)?;
            }
        }
        // Squash uses Git's composed message, with its normal cleanup rules.
    } else {
        return Err(RuleError::InvalidRebasePlan.into());
    }
    Ok(())
}

impl Repo {
    pub fn rebase_plan(&self, context: &CommitContext) -> Result<RebasePlan> {
        if context.availability(CommitAction::InteractiveRebase) != CommitAvailability::Enabled {
            return Err(RuleError::CommitUnavailable.into());
        }
        let bytes = self.read(&[
            "log",
            "--reverse",
            "--topo-order",
            "--encoding=UTF-8",
            COMMIT_DATE_FORMAT,
            &format!("{COMMIT_LOG_FORMAT}%x00%B"),
            "-z",
            &format!("{}..{}", context.commit.id, context.head),
            "--",
        ])?;
        let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
        let entries = fields
            .chunks_exact(7)
            .map(|fields| RebaseEntry {
                commit: commit_from_fields(&fields[..6]),
                action: RebaseAction::Pick,
                message: text(fields[6]).trim_end().to_owned(),
            })
            .collect();
        Ok(RebasePlan {
            base: context.commit.id.clone(),
            entries,
        })
    }

    pub(crate) fn validate_rebase_plan(
        &self,
        context: &CommitContext,
        plan: &RebasePlan,
    ) -> Result<()> {
        let invalid = || RuleError::InvalidRebasePlan;
        if plan.base != context.commit.id || plan.entries.is_empty() || !context.linear_rebase {
            return Err(invalid().into());
        }
        let expected: HashSet<_> = String::from_utf8_lossy(&self.read(&[
            "rev-list",
            &format!("{}..{}", plan.base, context.head),
            "--",
        ])?)
        .lines()
        .map(str::to_owned)
        .collect();
        let actual: HashSet<_> = plan
            .entries
            .iter()
            .map(|entry| entry.commit.id.clone())
            .collect();
        if expected != actual || plan.entries.len() != actual.len() {
            return Err(invalid().into());
        }
        let mut kept = false;
        for entry in &plan.entries {
            if (matches!(entry.action, RebaseAction::Squash | RebaseAction::Fixup) && !kept)
                || (entry.action == RebaseAction::Reword && entry.message.trim().is_empty())
            {
                return Err(invalid().into());
            }
            kept |= entry.action != RebaseAction::Drop;
        }
        Ok(())
    }

    pub(crate) fn start_interactive_rebase(
        &self,
        plan: &RebasePlan,
        editor: &GitEditor,
    ) -> Result<()> {
        self.clean_rebase_files()?;
        let directory = self.git_path("ritkraken-rebase")?;
        fs::create_dir(&directory)?;
        let mut todo = String::new();
        for entry in &plan.entries {
            todo.push_str(&format!("{} {}\n", entry.action.command(), entry.commit.id));
            if entry.action == RebaseAction::Reword {
                fs::write(
                    directory.join(format!("{}.message", entry.commit.id)),
                    &entry.message,
                )?;
            }
        }
        fs::write(directory.join("todo"), todo)?;
        let mut command = self.git();
        editor.configure(self, &mut command)?;
        let result = run_write(command.args([
            "-c",
            "rebase.updateRefs=false",
            "rebase",
            "--interactive",
            "--no-rebase-merges",
            "--no-autosquash",
            "--no-autostash",
            "--no-fork-point",
            &plan.base,
        ]));
        if self
            .sequence_kind()
            .is_ok_and(|sequence| sequence.is_none())
        {
            let _ = self.clean_rebase_files();
        }
        result
    }

    pub(crate) fn clean_rebase_files(&self) -> Result<()> {
        let directory = self.git_path("ritkraken-rebase")?;
        if !directory.exists() {
            return Ok(());
        }
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            return Err(RuleError::InvalidRebasePlan.into());
        }
        // Remove only the exact application-owned files, never recursively
        // delete a computed Git metadata path.
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "todo"
                || name.strip_suffix(".message").is_some_and(|id| {
                    matches!(id.len(), 40 | 64) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
            {
                fs::remove_file(entry.path())?;
            }
        }
        fs::remove_dir(directory)?;
        Ok(())
    }
}
