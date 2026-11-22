use std::{process::Command, thread};

use crate::{
    Commit, Repo, Result, RuleError,
    command::{checked, join, read_output, spawn},
    diff::{PATCH_OPTIONS, read_preview},
    history::{COMMIT_DATE_FORMAT, COMMIT_LOG_FORMAT, HISTORY_ORDER, parse_log},
    protocol::text,
};

/// A two-revision comparison: the commits each side has exclusively, their
/// merge base, and the patch the compare side would bring into the base.
pub struct RefComparison {
    /// `None` for unrelated histories.
    pub merge_base: Option<String>,
    /// Reachable from the base but not the compare side, newest first.
    pub only_base: Vec<Commit>,
    /// Reachable from the compare side but not the base, newest first.
    pub only_compare: Vec<Commit>,
    /// More than `limit` commits existed on that side.
    pub base_limited: bool,
    pub compare_limited: bool,
    /// `merge_base..compare` (or `base..compare` when unrelated);
    /// `None` when the patch exceeds the preview limit.
    pub patch: Option<Vec<u8>>,
}

impl Repo {
    /// Compares `base` against `compare`. The revisions may be full ref names
    /// or `HEAD`; annotated tags peel to their commit.
    pub fn compare_refs(&self, base: &str, compare: &str, limit: usize) -> Result<RefComparison> {
        let base = self.commit_revision(base)?;
        let compare = self.commit_revision(compare)?;
        thread::scope(|scope| {
            let only_base = spawn(scope, || self.commits_exclusive(&compare, &base, limit))?;
            let only_compare = spawn(scope, || self.commits_exclusive(&base, &compare, limit))?;
            let merge_base = join(spawn(scope, || self.merge_base(&base, &compare))?)?;
            // Unrelated histories show the direct base->compare diff.
            let from = merge_base.clone().unwrap_or_else(|| base.clone());
            let compare_commit = compare.clone();
            let patch = spawn(scope, move || self.diff_revisions(&from, &compare_commit))?;
            let (only_base, base_limited) = join(only_base)?;
            let (only_compare, compare_limited) = join(only_compare)?;
            Ok(RefComparison {
                merge_base,
                only_base,
                only_compare,
                base_limited,
                compare_limited,
                patch: join(patch)?,
            })
        })
    }

    /// The patch a single commit introduced: against its first parent, or the
    /// root diff for an initial commit. `None` over the preview limit.
    pub fn commit_diff(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let commit = self.commit_at(id)?;
        let mut git = self.git();
        match commit.parents.first() {
            Some(parent) => {
                git.arg("diff").args(PATCH_OPTIONS).args(["-M", parent, id]);
            }
            None => {
                git.args(["diff-tree", "--root", "--no-commit-id", "-r", "-p", "-M"])
                    .args(PATCH_OPTIONS)
                    .arg(id);
            }
        }
        git.arg("--");
        read_preview(&mut git, false)
    }

    /// Commits reachable from `include` but not `exclude`, newest first, and
    /// whether more than `limit` commits existed.
    fn commits_exclusive(
        &self,
        exclude: &str,
        include: &str,
        limit: usize,
    ) -> Result<(Vec<Commit>, bool)> {
        let mut commits = parse_log(&self.read(&[
            "log",
            HISTORY_ORDER,
            COMMIT_DATE_FORMAT,
            "--encoding=UTF-8",
            COMMIT_LOG_FORMAT,
            "-z",
            &format!("--max-count={}", limit.saturating_add(1)),
            include,
            &format!("^{exclude}"),
            "--",
        ])?);
        let limited = commits.len() > limit;
        commits.truncate(limit);
        Ok((commits, limited))
    }

    fn merge_base(&self, base: &str, compare: &str) -> Result<Option<String>> {
        let output = read_output(self.git().args(["merge-base", base, compare]))?;
        if output.status.code() == Some(1) && output.stdout.is_empty() {
            return Ok(None);
        }
        checked(output).map(|bytes| Some(text(&bytes).trim().to_owned()))
    }

    fn diff_revisions(&self, from: &str, to: &str) -> Result<Option<Vec<u8>>> {
        let mut git: Command = self.git();
        git.arg("diff")
            .args(PATCH_OPTIONS)
            .args(["-M", from, to, "--"]);
        read_preview(&mut git, false)
    }

    /// Peels a revision (ref name or `HEAD`) to the commit it names.
    fn commit_revision(&self, revision: &str) -> Result<String> {
        let output = read_output(
            self.git()
                .args(["rev-parse", "--verify", "--quiet"])
                .arg(format!("{revision}^{{commit}}")),
        )?;
        if !output.status.success() {
            return Err(RuleError::InvalidCommit.into());
        }
        Ok(text(&output.stdout).trim().to_owned())
    }
}
