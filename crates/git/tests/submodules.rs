mod support;

use std::{fs, path::Path};

use ritkraken_git::{Change, FileTarget, Repo, Scope};
use support::commits::commit_file;
use support::{command, fixture, git_output};

fn head(repo: &Repo) -> String {
    support::commits::head(repo)
}

fn sub_remote() -> (tempfile::TempDir, Repo) {
    let directory = tempfile::Builder::new()
        .prefix("ritkraken-git-sub-")
        .tempdir()
        .unwrap();
    let repo = Repo::init(directory.path(), Some("main")).unwrap();
    for (key, value) in [
        ("user.name", "RitKraken Test"),
        ("user.email", "ritkraken-test@example.invalid"),
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
    ] {
        git_output(command(Some(repo.root())).args(["config", "--local", key, value]));
    }
    commit_file(&repo, "lib.txt", "one\n", "initial library state");
    (directory, repo)
}

fn add_submodule(repo: &Repo, remote: &Repo) {
    git_output(command(Some(repo.root())).args([
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--",
        &remote.root().to_string_lossy(),
        "lib",
    ]));
    repo.commit("Add lib submodule", "").unwrap();
}

fn target(repo: &Repo, scope: Scope) -> FileTarget {
    let status = repo.status().unwrap();
    let files = match scope {
        Scope::Worktree => status.unstaged,
        Scope::Index => status.staged,
        _ => unreachable!(),
    };
    FileTarget {
        scope,
        change: files
            .into_iter()
            .find(|change| change.path == Path::new("lib"))
            .unwrap(),
    }
}

fn change(repo: &Repo, file: &FileTarget) -> ritkraken_git::SubmoduleChange {
    let patch = repo.patch(file).unwrap();
    repo.submodule_change(file, &patch).unwrap().unwrap()
}

#[test]
fn commit_scope_adds_detect_old_none_new_head() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let sub_head = head(&remote);
    add_submodule(&repo, &remote);
    let id = head(&repo);
    let detail_change = repo
        .committed_files(&id)
        .unwrap()
        .into_iter()
        .find(|change| change.path == Path::new("lib"))
        .unwrap();
    let file = FileTarget {
        scope: Scope::Commit {
            id,
            parent: None,
        },
        change: detail_change,
    };
    let patch = repo.patch(&file).unwrap();
    let change = repo.submodule_change(&file, &patch).unwrap().unwrap();
    assert_eq!(change.old, None);
    assert_eq!(change.new.as_deref(), Some(sub_head.as_str()));
    assert!(!change.modified);
    assert!(!change.untracked);
}

#[test]
fn index_scope_pointer_change_reports_both_commits() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let old_head = head(&remote);
    add_submodule(&repo, &remote);
    let new_head = commit_file(&remote, "lib.txt", "two\n", "next library state");
    git_output(command(Some(&repo.root().join("lib"))).args(["fetch", "origin", "main"]));
    git_output(command(Some(&repo.root().join("lib"))).args(["checkout", &new_head]));
    repo.stage(Some(&Change {
        path: "lib".into(),
        previous: None,
        code: b'M',
    }))
    .unwrap();
    let file = target(&repo, Scope::Index);
    let change = change(&repo, &file);
    assert_eq!(change.old.as_deref(), Some(old_head.as_str()));
    assert_eq!(change.new.as_deref(), Some(new_head.as_str()));
}

#[test]
fn worktree_pointer_change_reports_index_and_worktree_commits() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let old_head = head(&remote);
    add_submodule(&repo, &remote);
    let new_head = commit_file(&remote, "lib.txt", "two\n", "next library state");
    git_output(command(Some(&repo.root().join("lib"))).args(["fetch", "origin", "main"]));
    git_output(command(Some(&repo.root().join("lib"))).args(["checkout", &new_head]));
    let file = target(&repo, Scope::Worktree);
    let change = change(&repo, &file);
    assert_eq!(change.old.as_deref(), Some(old_head.as_str()));
    assert_eq!(change.new.as_deref(), Some(new_head.as_str()));
}

#[test]
fn worktree_modified_only_marks_modified_without_pointer_change() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let sub_head = head(&remote);
    add_submodule(&repo, &remote);
    fs::write(repo.root().join("lib/lib.txt"), "edited\n").unwrap();
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    };
    let change = change(&repo, &file);
    assert!(change.modified);
    assert!(!change.untracked);
    assert_eq!(change.old.as_deref(), Some(sub_head.as_str()));
    assert_eq!(change.new.as_deref(), Some(sub_head.as_str()));
}

#[test]
fn worktree_untracked_only_has_empty_patch_but_marks_untracked() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let sub_head = head(&remote);
    add_submodule(&repo, &remote);
    fs::write(repo.root().join("lib/scratch.txt"), "new\n").unwrap();
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    };
    let patch = repo.patch(&file).unwrap();
    assert!(patch.bytes().is_empty());
    let change = repo.submodule_change(&file, &patch).unwrap().unwrap();
    assert!(change.untracked);
    assert!(!change.modified);
    assert_eq!(change.old.as_deref(), Some(sub_head.as_str()));
    assert_eq!(change.new.as_deref(), Some(sub_head.as_str()));
}

#[test]
fn regular_text_file_returns_none() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "notes.txt", "hello\n", "Add notes");
    fs::write(repo.root().join("notes.txt"), "hello\nworld\n").unwrap();
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "notes.txt".into(),
            previous: None,
            code: b'M',
        },
    };
    let patch = repo.patch(&file).unwrap();
    assert!(repo.submodule_change(&file, &patch).unwrap().is_none());
}

#[test]
fn text_file_whose_content_is_a_subproject_line_returns_none() {
    let (_directory, repo) = fixture();
    let fake = "a".repeat(40);
    commit_file(
        &repo,
        "marker.txt",
        &format!("Subproject commit {fake}\n"),
        "Add marker",
    );
    let other = "b".repeat(40);
    fs::write(
        repo.root().join("marker.txt"),
        format!("Subproject commit {other}\n"),
    )
    .unwrap();
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "marker.txt".into(),
            previous: None,
            code: b'M',
        },
    };
    let patch = repo.patch(&file).unwrap();
    assert!(repo.submodule_change(&file, &patch).unwrap().is_none());
}

#[test]
fn diff_submodule_log_config_still_detected() {
    let (_directory, repo) = fixture();
    git_output(command(Some(repo.root())).args([
        "config",
        "--local",
        "diff.submodule",
        "log",
    ]));
    let (_remote_dir, remote) = sub_remote();
    let old_head = head(&remote);
    add_submodule(&repo, &remote);
    let new_head = commit_file(&remote, "lib.txt", "two\n", "next library state");
    git_output(command(Some(&repo.root().join("lib"))).args(["fetch", "origin", "main"]));
    git_output(command(Some(&repo.root().join("lib"))).args(["checkout", &new_head]));
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    };
    let change = change(&repo, &file);
    assert_eq!(change.old.as_deref(), Some(old_head.as_str()));
    assert_eq!(change.new.as_deref(), Some(new_head.as_str()));
}

#[test]
fn submodule_info_initialized_resolves_subjects() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    let new_head = commit_file(&remote, "lib.txt", "two\n", "next library state");
    git_output(command(Some(&repo.root().join("lib"))).args(["fetch", "origin", "main"]));
    git_output(command(Some(&repo.root().join("lib"))).args(["checkout", &new_head]));
    let file = target(&repo, Scope::Worktree);
    let change = change(&repo, &file);
    let info = repo.submodule_info(Path::new("lib"), change);
    assert!(info.initialized);
    assert_eq!(info.path, repo.root().join("lib"));
    assert_eq!(info.old_subject.as_deref(), Some("initial library state"));
    assert_eq!(info.new_subject.as_deref(), Some("next library state"));
}

#[test]
fn submodule_info_uninitialized_reports_not_initialized() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let sub_head = head(&remote);
    add_submodule(&repo, &remote);
    git_output(command(Some(repo.root())).args([
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "deinit",
        "-f",
        "--",
        "lib",
    ]));
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    };
    let patch = repo.patch(&file).unwrap();
    let change = repo
        .submodule_change(&file, &patch)
        .unwrap()
        .unwrap_or(ritkraken_git::SubmoduleChange {
            old: None,
            new: Some(sub_head),
            modified: false,
            untracked: false,
        });
    let info = repo.submodule_info(Path::new("lib"), change);
    assert!(!info.initialized);
    assert_eq!(info.old_subject, None);
    assert_eq!(info.new_subject, None);
}

#[test]
fn patch_uses_submodule_short_format() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    fs::write(repo.root().join("lib/lib.txt"), "edited\n").unwrap();
    let file = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    };
    let patch = repo.patch(&file).unwrap();
    let bytes = String::from_utf8_lossy(patch.bytes());
    assert!(bytes.contains("Subproject commit"));
    assert!(bytes.contains("-dirty"));
}
