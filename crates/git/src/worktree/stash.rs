use std::{path::Path, process::Command};

use crate::{
    Change, FileTarget, Repo, Result, RuleError, Scope,
    command::{run_read, run_with_input, run_write},
    diff::PATCH_OPTIONS,
    protocol::text,
};

use super::append_paths;

impl Repo {
    pub fn stash_file(&self, change: &Change) -> Result<()> {
        let status = self.status()?;
        let staged = status.staged.iter().find(|entry| entry.path == change.path);
        let file = FileTarget {
            scope: if staged.is_some() {
                Scope::Index
            } else {
                Scope::Worktree
            },
            change: staged.unwrap_or(change).clone(),
        };
        let expected = self.prepare_discard(&file)?;
        let head = self
            .resolve_ref("HEAD")?
            .ok_or(RuleError::UnstageableSelection)?;
        let untracked = staged.is_none() && change.code == b'?';
        let directory = tempfile::Builder::new()
            .prefix("ritkraken-stash-")
            .tempdir()?;
        let index = directory.path().join("index");
        let command = || temporary_index(self, &index);

        // Build the index parent from HEAD plus this file's staged entries.
        // Git's path-limited stash otherwise retains unrelated staged entries.
        run_write(command().args(["read-tree", &head]))?;
        let mut remove = command();
        remove.args(["update-index", "--force-remove", "--"]);
        append_paths(&mut remove, &file.change);
        run_write(&mut remove)?;
        let mut entries = self.git_with_literal_paths();
        entries.args(["ls-files", "--stage", "-z", "--"]);
        append_paths(&mut entries, &file.change);
        let entries = run_read(&mut entries)?;
        run_with_input(
            command().args(["update-index", "-z", "--index-info"]),
            &[&entries],
        )?;
        let index_tree = text(&run_read(command().arg("write-tree"))?)
            .trim()
            .to_owned();
        let index_commit = stash_commit(self, &index_tree, &[&head], "File stash index\n")?;

        // update-index addresses literal paths, including a removed rename source.
        // Its temporary index keeps the real index and other files untouched.
        if untracked {
            run_write(command().args(["read-tree", "--empty"]))?;
        } else {
            run_write(command().args(["read-tree", &head]))?;
        }
        let mut working = command();
        working.args(["update-index", "--add", "--remove", "--"]);
        append_paths(&mut working, &file.change);
        run_write(&mut working)?;
        let working_tree = text(&run_read(command().arg("write-tree"))?)
            .trim()
            .to_owned();
        let untracked_commit = if untracked {
            Some(stash_commit(
                self,
                &working_tree,
                &[],
                "File stash untracked\n",
            )?)
        } else {
            None
        };
        let working_tree = if untracked {
            text(&self.read(&["rev-parse", &format!("{head}^{{tree}}")])?)
                .trim()
                .to_owned()
        } else {
            working_tree
        };
        let mut parents = vec![head.as_str(), index_commit.as_str()];
        parents.extend(untracked_commit.as_deref());
        let summary = format!("File stash: {}", change.path.display());
        let message = format!("{summary}\n\nRitKraken-File-Stash: 1\n");
        let commit = stash_commit(self, &working_tree, &parents, &message)?;
        self.verify_discard(&expected)?;
        run_write(self.git().args(["stash", "store", "-m", &summary, &commit]))?;

        // Save the stash before cleanup, so a failed cleanup retains every version.
        self.verify_discard(&expected)?;
        if untracked {
            return self.discard_file(&expected);
        }
        let mut cleanup = self.git_with_literal_paths();
        cleanup.args(["restore", "--source=HEAD", "--staged", "--worktree", "--"]);
        append_paths(&mut cleanup, &file.change);
        run_write(&mut cleanup)
    }

    pub(super) fn pop_file_stash(&self) -> Result<bool> {
        let Some(stash) = self.resolve_ref("refs/stash")? else {
            return Ok(false);
        };
        let marker = self.read(&[
            "show",
            "--no-patch",
            "--no-show-signature",
            "--format=%(trailers:key=RitKraken-File-Stash,valueonly)",
            &stash,
            "--",
        ])?;
        if marker.trim_ascii() != b"1" {
            return Ok(false);
        }
        let base = self
            .resolve_ref(&format!("{stash}^1"))?
            .ok_or(RuleError::StaleFile)?;
        let index = self
            .resolve_ref(&format!("{stash}^2"))?
            .ok_or(RuleError::StaleFile)?;
        let mut working = snapshot_patch(self, &base, &stash)?;
        let staged = snapshot_patch(self, &base, &index)?;
        if let Some(untracked) = self.resolve_ref(&format!("{stash}^3"))? {
            let bytes = run_read(
                self.git()
                    .args(["diff-tree", "--root", "--no-commit-id", "-r", "-p"])
                    .args(PATCH_OPTIONS)
                    .args(["--binary", &untracked, "--"]),
            )?;
            working.extend(bytes);
        }
        apply_stash_patch(self, &working, &["--check"])?;
        apply_stash_patch(self, &staged, &["--check", "--cached"])?;
        apply_stash_patch(self, &working, &[])?;
        apply_stash_patch(self, &staged, &["--cached"])?;
        if self.resolve_ref("refs/stash")?.as_deref() != Some(stash.as_str()) {
            return Err(RuleError::StashListChanged.into());
        }
        run_write(self.git().args(["stash", "drop", "stash@{0}"]))?;
        Ok(true)
    }
}

fn snapshot_patch(repo: &Repo, from: &str, to: &str) -> Result<Vec<u8>> {
    run_read(
        repo.git()
            .arg("diff")
            .args(PATCH_OPTIONS)
            .args(["--binary", from, to, "--"]),
    )
}

fn apply_stash_patch(repo: &Repo, patch: &[u8], options: &[&str]) -> Result<()> {
    if patch.is_empty() {
        return Ok(());
    }
    run_with_input(
        repo.git().args(["apply", "--binary"]).args(options),
        &[patch],
    )
    .map(drop)
}

fn temporary_index(repo: &Repo, path: &Path) -> Command {
    let mut command = repo.git_with_literal_paths();
    command.env("GIT_INDEX_FILE", path);
    command
}

fn stash_commit(repo: &Repo, tree: &str, parents: &[&str], message: &str) -> Result<String> {
    let mut command = repo.git();
    command.args(["commit-tree", "--no-gpg-sign", tree]);
    for parent in parents {
        command.args(["-p", parent]);
    }
    run_with_input(&mut command, &[message.as_bytes()]).map(|bytes| text(&bytes).trim().to_owned())
}
