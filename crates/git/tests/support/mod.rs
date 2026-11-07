use std::{path::Path, process::Command};

use ritkraken_git::Repo;

pub fn command(directory: Option<&Path>) -> Command {
    let mut git = Command::new("git");
    if let Some(directory) = directory {
        git.arg("-C").arg(directory);
    }
    git
}

pub fn git_output(command: &mut Command) -> Vec<u8> {
    let output = command.output().expect("run Git for the test fixture");
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

pub fn fixture() -> (tempfile::TempDir, Repo) {
    let directory = tempfile::Builder::new()
        .prefix("ritkraken-git-")
        .tempdir()
        .unwrap();
    let repo = Repo::init(directory.path(), Some("main")).unwrap();
    for (key, value) in [
        ("user.name", "RitKraken Test"),
        ("user.email", "ritkraken-test@example.invalid"),
        ("core.autocrlf", "false"),
        ("core.hooksPath", ".git/disabled-hooks"),
        ("commit.gpgsign", "false"),
        ("protocol.file.allow", "always"),
    ] {
        git_output(command(Some(repo.root())).args(["config", "--local", key, value]));
    }
    (directory, repo)
}
