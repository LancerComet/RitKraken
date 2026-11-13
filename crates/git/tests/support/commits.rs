#![allow(dead_code)]

use super::{command, git_output};
use ritkraken_git::{CommitOperation, GitEditor, Repo};
use std::{fs, path::Path};

pub fn head(repo: &Repo) -> String {
    git(repo, &["rev-parse", "HEAD"]).trim().to_owned()
}

pub fn git(repo: &Repo, arguments: &[&str]) -> String {
    String::from_utf8(git_output(command(Some(repo.root())).args(arguments))).unwrap()
}

pub fn commit_file(repo: &Repo, path: &str, contents: &str, message: &str) -> String {
    fs::write(repo.root().join(path), contents).unwrap();
    git(repo, &["add", "--", path]);
    repo.commit(message, "").unwrap();
    head(repo)
}

pub fn apply(repo: &Repo, id: &str, operation: CommitOperation) {
    let context = repo.commit_context(id).unwrap();
    let prepared = repo.prepare_commit_operation(&context, operation).unwrap();
    repo.apply_commit_operation(&prepared, &editor()).unwrap();
}

pub fn editor() -> GitEditor {
    // Git invokes editor commands through its own shell on Windows and Unix.
    GitEditor {
        sequence: "cat \"$RITKRAKEN_REBASE_DATA/todo\" >".into(),
        message: "true".into(),
    }
}

pub fn reword_editor(directory: &Path) -> GitEditor {
    let script = directory.join("message editor.sh");
    fs::write(&script, "destination=$1\naction=\nid=\nwhile read -r next_action next_id rest; do\n  if test -n \"$next_action\"; then action=$next_action; id=$next_id; fi\ndone < \"$RITKRAKEN_REBASE_DONE\"\ncase \"$action\" in reword|squash|fixup) if test -f \"$RITKRAKEN_REBASE_DATA/$id.message\"; then cp \"$RITKRAKEN_REBASE_DATA/$id.message\" \"$destination\"; fi ;; esac\n").unwrap();
    let path = script.to_string_lossy().replace('\'', "'\\''");
    GitEditor {
        message: format!("sh '{path}'"),
        ..editor()
    }
}
