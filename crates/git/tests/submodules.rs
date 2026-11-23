mod support;

use std::{fs, path::Path};

use ritkraken_git::{Change, Error, FileTarget, Repo, RuleError, Scope};
use support::commits::{commit_file, git};
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
        .changes
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

fn worktree_lib() -> FileTarget {
    FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "lib".into(),
            previous: None,
            code: b'M',
        },
    }
}

fn lib_head(repo: &Repo) -> String {
    String::from_utf8(git_output(
        command(Some(&repo.root().join("lib"))).args(["rev-parse", "HEAD"]),
    ))
    .unwrap()
    .trim()
    .to_owned()
}

fn checkout_new_head(repo: &Repo, new_head: &str) {
    git_output(command(Some(&repo.root().join("lib"))).args(["fetch", "origin", "main"]));
    git_output(command(Some(&repo.root().join("lib"))).args(["checkout", new_head]));
}

#[test]
fn discarding_a_submodule_change_is_refused() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    let new_head = commit_file(&remote, "lib.txt", "two\n", "next library state");
    checkout_new_head(&repo, &new_head);
    let error = repo.prepare_discard(&worktree_lib()).unwrap_err();
    assert!(matches!(
        error,
        Error::Rule(RuleError::SubmoduleDiscard)
    ));
    assert_eq!(lib_head(&repo), new_head);
}

#[test]
fn a_mixed_selection_refuses_discard_and_leaves_files_untouched() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    commit_file(&repo, "notes.txt", "base\n", "Add notes");
    fs::write(repo.root().join("notes.txt"), "edited\n").unwrap();
    let notes = FileTarget {
        scope: Scope::Worktree,
        change: Change {
            path: "notes.txt".into(),
            previous: None,
            code: b'M',
        },
    };
    let lib = worktree_lib();
    assert!(repo.prepare_discard(&notes).is_ok());
    assert!(matches!(
        repo.prepare_discard(&lib).unwrap_err(),
        Error::Rule(RuleError::SubmoduleDiscard)
    ));
    let prepared: ritkraken_git::Result<Vec<_>> = [notes, lib]
        .iter()
        .map(|file| repo.prepare_discard(file))
        .collect();
    assert!(prepared.is_err());
    assert_eq!(fs::read(repo.root().join("notes.txt")).unwrap(), b"edited\n");
}

#[test]
fn update_submodule_restores_the_recorded_commit() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let recorded = commit_file(&remote, "lib.txt", "two\n", "next library state");
    add_submodule(&repo, &remote);
    let first = git(&remote, &["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_owned();
    git_output(command(Some(&repo.root().join("lib"))).args([
        "checkout", "-b", "work", &first,
    ]));
    assert_eq!(
        repo.status().unwrap().unstaged[0].path,
        Path::new("lib")
    );
    repo.update_submodule(Path::new("lib")).unwrap();
    assert_eq!(lib_head(&repo), recorded);
    assert!(repo.status().unwrap().unstaged.is_empty());
}

#[test]
fn update_submodule_initializes_a_deinitialized_submodule() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let recorded = head(&remote);
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
    repo.update_submodule(Path::new("lib")).unwrap();
    assert_eq!(lib_head(&repo), recorded);
    let file = worktree_lib();
    let patch = repo.patch(&file).unwrap();
    let change = repo
        .submodule_change(&file, &patch)
        .unwrap()
        .unwrap_or(ritkraken_git::SubmoduleChange {
            old: None,
            new: None,
            modified: false,
            untracked: false,
        });
    let info = repo.submodule_info(Path::new("lib"), change);
    assert!(info.initialized);
}

#[test]
fn update_submodule_refuses_when_head_would_be_dangling() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    let lib = repo.root().join("lib");
    git_output(command(Some(&lib)).args(["checkout", "--detach"]));
    fs::write(lib.join("lib.txt"), "detached work\n").unwrap();
    git_output(command(Some(&lib)).args(["add", "lib.txt"]));
    git_output(command(Some(&lib)).args([
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "dangling work",
    ]));
    let dangling = lib_head(&repo);
    let error = repo.update_submodule(Path::new("lib")).unwrap_err();
    assert!(matches!(
        error,
        Error::Rule(RuleError::UnreachableSubmoduleCommit)
    ));
    assert_eq!(lib_head(&repo), dangling);
}

#[test]
fn update_submodule_preserves_non_conflicting_local_edits() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let middle = commit_file(&remote, "other.txt", "other\n", "Add other file");
    let recorded = commit_file(&remote, "lib.txt", "two\n", "next library state");
    add_submodule(&repo, &remote);
    let lib = repo.root().join("lib");
    git_output(command(Some(&lib)).args(["checkout", "-b", "work", &middle]));
    fs::write(lib.join("other.txt"), "local edit\n").unwrap();
    repo.update_submodule(Path::new("lib")).unwrap();
    assert_eq!(fs::read(lib.join("other.txt")).unwrap(), b"local edit\n");
    assert_eq!(lib_head(&repo), recorded);
}

#[test]
fn update_submodule_refuses_conflicting_local_edits() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    commit_file(&remote, "lib.txt", "two\n", "next library state");
    add_submodule(&repo, &remote);
    let first = git(&remote, &["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_owned();
    let lib = repo.root().join("lib");
    git_output(command(Some(&lib)).args(["checkout", "-b", "work", &first]));
    fs::write(lib.join("lib.txt"), "conflicting edit\n").unwrap();
    assert!(repo.update_submodule(Path::new("lib")).is_err());
    assert_eq!(
        fs::read(lib.join("lib.txt")).unwrap(),
        b"conflicting edit\n"
    );
    assert_eq!(lib_head(&repo), first);
}

#[test]
fn update_submodule_clones_a_missing_submodule() {
    unsafe {
        std::env::set_var("GIT_CONFIG_COUNT", "1");
        std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
        std::env::set_var("GIT_CONFIG_VALUE_0", "always");
    }
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let recorded = head(&remote);
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
    fs::remove_dir_all(repo.root().join(".git/modules/lib")).unwrap();
    repo.update_submodule(Path::new("lib")).unwrap();
    assert_eq!(lib_head(&repo), recorded);
    let file = worktree_lib();
    let patch = repo.patch(&file).unwrap();
    let change = repo
        .submodule_change(&file, &patch)
        .unwrap()
        .unwrap_or(ritkraken_git::SubmoduleChange {
            old: None,
            new: None,
            modified: false,
            untracked: false,
        });
    assert!(repo.submodule_info(Path::new("lib"), change).initialized);
}

fn add_named_submodule(repo: &Repo, remote: &Repo, name: &str) {
    git_output(command(Some(repo.root())).args([
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--",
        &remote.root().to_string_lossy(),
        name,
    ]));
}

fn head_in(repo: &Repo, path: &str) -> String {
    String::from_utf8(git_output(
        command(Some(&repo.root().join(path))).args(["rev-parse", "HEAD"]),
    ))
    .unwrap()
    .trim()
    .to_owned()
}

fn commit_target(id: &str, parent: Option<String>, path: &str, code: u8) -> FileTarget {
    FileTarget {
        scope: Scope::Commit {
            id: id.to_owned(),
            parent,
        },
        change: Change {
            path: path.into(),
            previous: None,
            code,
        },
    }
}

#[test]
fn status_marks_submodule_directories() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    commit_file(&repo, "notes.txt", "base\n", "Add notes");
    let lib = repo.root().join("lib");
    fs::write(lib.join("extra.txt"), "x\n").unwrap();
    git_output(command(Some(&lib)).args(["add", "extra.txt"]));
    git_output(command(Some(&lib)).args([
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "extra",
    ]));
    fs::write(repo.root().join("notes.txt"), "edited\n").unwrap();
    let nested = repo.root().join("nested");
    fs::create_dir(&nested).unwrap();
    git_output(command(Some(&nested)).args(["init", "-q"]));
    let status = repo.status().unwrap();
    assert!(status.submodules.contains(Path::new("lib")));
    assert!(!status.submodules.contains(Path::new("notes.txt")));
    assert!(!status.submodules.contains(Path::new("nested")));
    assert!(
        status
            .unstaged
            .iter()
            .any(|change| change.code == b'?' && change.path.starts_with("nested"))
    );
}

#[test]
fn detail_marks_gitlinks_and_keeps_changes() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    let snapshot = repo.snapshot(None).unwrap();
    let commit = snapshot
        .commits
        .iter()
        .find(|commit| commit.subject == "Add lib submodule")
        .unwrap();
    let detail = repo.detail(commit).unwrap();
    assert!(detail.submodules.contains(Path::new("lib")));
    assert!(!detail.submodules.contains(Path::new(".gitmodules")));
    let lib = detail
        .changes
        .iter()
        .find(|change| change.path == Path::new("lib"))
        .unwrap();
    assert_eq!(lib.code, b'A');
    let modules = detail
        .changes
        .iter()
        .find(|change| change.path == Path::new(".gitmodules"))
        .unwrap();
    assert_eq!(modules.code, b'A');
    assert!(modules.previous.is_none());
}

#[test]
fn detail_preserves_rename_status_and_reports_no_submodules() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "old.txt", "contents\n", "Add file");
    fs::rename(repo.root().join("old.txt"), repo.root().join("new.txt")).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Rename file", "").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    let detail = repo.detail(&snapshot.commits[0]).unwrap();
    let change = &detail.changes[0];
    assert_eq!(change.code, b'R');
    assert_eq!(change.previous.as_deref(), Some(Path::new("old.txt")));
    assert_eq!(change.path, Path::new("new.txt"));
    assert!(detail.submodules.is_empty());
}

#[test]
fn committed_files_marks_gitlinks_and_lists_everything() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let files = repo.committed_files(&head).unwrap();
    assert!(files.submodules.contains(Path::new("lib")));
    assert!(!files.submodules.contains(Path::new(".gitmodules")));
    assert!(
        files
            .changes
            .iter()
            .any(|change| change.path == Path::new("lib") && change.code == b' ')
    );
    assert!(
        files
            .changes
            .iter()
            .any(|change| change.path == Path::new(".gitmodules"))
    );
}

#[test]
fn commit_scope_unchanged_submodule_reports_current_commit() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    commit_file(&repo, "notes.txt", "n\n", "Add notes");
    let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let recorded = git(&repo, &["rev-parse", ":lib"]).trim().to_owned();
    let target = commit_target(&head, None, "lib", b' ');
    let patch = repo.patch(&target).unwrap();
    let change = repo.submodule_change(&target, &patch).unwrap().unwrap();
    assert_eq!(change.old.as_deref(), Some(recorded.as_str()));
    assert_eq!(change.new.as_deref(), Some(recorded.as_str()));
    assert!(!change.modified && !change.untracked);
}

#[test]
fn commit_scope_unchanged_regular_file_is_not_a_submodule() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_submodule(&repo, &remote);
    commit_file(&repo, "notes.txt", "n\n", "Add notes");
    let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let target = commit_target(&head, None, ".gitmodules", b' ');
    let patch = repo.patch(&target).unwrap();
    assert!(repo.submodule_change(&target, &patch).unwrap().is_none());
}

fn move_submodule(repo: &Repo, name: &str, new_head: &str, branch: Option<&str>) {
    let dir = repo.root().join(name);
    git_output(command(Some(&dir)).args(["fetch", "origin", "main"]));
    match branch {
        Some(branch) => git_output(command(Some(&dir)).args([
            "checkout", "-b", branch, new_head,
        ])),
        None => git_output(command(Some(&dir)).args(["checkout", "--detach", new_head])),
    };
}

#[test]
fn update_submodules_restores_every_moved_submodule() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_named_submodule(&repo, &remote, "lib");
    add_named_submodule(&repo, &remote, "tools");
    repo.commit("Add submodules", "").unwrap();
    let recorded = head(&remote);
    let new_head = commit_file(&remote, "lib.txt", "three\n", "third library state");
    for name in ["lib", "tools"] {
        move_submodule(&repo, name, &new_head, Some("work"));
    }
    repo.update_submodules(None).unwrap();
    assert_eq!(head_in(&repo, "lib"), recorded);
    assert_eq!(head_in(&repo, "tools"), recorded);
    assert!(repo.status().unwrap().unstaged.is_empty());
}

#[test]
fn update_submodules_refuses_everything_when_one_head_is_dangling() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_named_submodule(&repo, &remote, "lib");
    add_named_submodule(&repo, &remote, "tools");
    repo.commit("Add submodules", "").unwrap();
    let new_head = commit_file(&remote, "lib.txt", "three\n", "third library state");
    move_submodule(&repo, "lib", &new_head, Some("work"));
    move_submodule(&repo, "tools", &new_head, None);
    let tools = repo.root().join("tools");
    fs::write(tools.join("lib.txt"), "dangling work\n").unwrap();
    git_output(command(Some(&tools)).args(["add", "lib.txt"]));
    git_output(command(Some(&tools)).args([
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "dangling",
    ]));
    let dangling = head_in(&repo, "tools");
    let error = repo.update_submodules(None).unwrap_err();
    assert!(matches!(
        error,
        Error::Rule(RuleError::UnreachableSubmoduleCommit)
    ));
    assert_eq!(head_in(&repo, "lib"), new_head);
    assert_eq!(head_in(&repo, "tools"), dangling);
}

#[test]
fn update_submodules_updates_only_the_listed_paths() {
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    add_named_submodule(&repo, &remote, "lib");
    add_named_submodule(&repo, &remote, "tools");
    repo.commit("Add submodules", "").unwrap();
    let recorded = head(&remote);
    let new_head = commit_file(&remote, "lib.txt", "three\n", "third library state");
    for name in ["lib", "tools"] {
        move_submodule(&repo, name, &new_head, Some("work"));
    }
    repo.update_submodules(Some(&[std::path::PathBuf::from("lib")]))
        .unwrap();
    assert_eq!(head_in(&repo, "lib"), recorded);
    assert_eq!(head_in(&repo, "tools"), new_head);
}

#[test]
fn init_submodules_recursive_initializes_a_fresh_clone() {
    unsafe {
        std::env::set_var("GIT_CONFIG_COUNT", "1");
        std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
        std::env::set_var("GIT_CONFIG_VALUE_0", "always");
    }
    let (_directory, repo) = fixture();
    let (_remote_dir, remote) = sub_remote();
    let recorded = head(&remote);
    add_submodule(&repo, &remote);
    let clone_dir = tempfile::Builder::new()
        .prefix("ritkraken-git-clone-")
        .tempdir()
        .unwrap();
    let cloned = Repo::clone(
        &repo.root().to_string_lossy(),
        &clone_dir.path().join("copy"),
    )
    .unwrap();
    assert!(!clone_dir.path().join("copy/lib/lib.txt").exists());
    cloned.init_submodules_recursive().unwrap();
    assert_eq!(
        String::from_utf8(git_output(
            command(Some(&clone_dir.path().join("copy/lib")))
                .args(["rev-parse", "HEAD"])
        ))
        .unwrap()
        .trim(),
        recorded
    );
}
