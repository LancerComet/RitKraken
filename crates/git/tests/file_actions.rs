mod support;

use std::{fs, path::Path};

use ritkraken_git::{Change, Error, FileTarget, IgnoreRule, Repo, RuleError, Scope};
use support::{command, fixture, git_output};

fn commit_files(repo: &Repo, files: &[(&str, &[u8])]) {
    for (path, contents) in files {
        fs::write(repo.root().join(path), contents).unwrap();
    }
    repo.stage(None).unwrap();
    repo.commit("Baseline", "").unwrap();
}

fn target(repo: &Repo, scope: Scope, name: &str) -> FileTarget {
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
            .find(|change| change.path == Path::new(name))
            .unwrap(),
    }
}

fn head(repo: &Repo) -> String {
    String::from_utf8(git_output(
        command(Some(repo.root())).args(["rev-parse", "HEAD"]),
    ))
    .unwrap()
    .trim()
    .to_owned()
}

#[test]
fn discard_refuses_external_edits_and_preserves_the_staged_version() {
    let (_directory, repo) = fixture();
    commit_files(
        &repo,
        &[("selected [1].txt", b"base\n"), ("other.txt", b"other\n")],
    );
    let selected = repo.root().join("selected [1].txt");
    fs::write(&selected, b"staged\n").unwrap();
    repo.stage(Some(&Change {
        path: "selected [1].txt".into(),
        previous: None,
        code: b'M',
    }))
    .unwrap();
    fs::write(&selected, b"working\n").unwrap();
    fs::write(repo.root().join("other.txt"), b"unrelated\n").unwrap();
    let file = target(&repo, Scope::Worktree, "selected [1].txt");
    let expected = repo.prepare_discard(&file).unwrap();
    fs::write(&selected, b"external edit\n").unwrap();
    assert!(matches!(
        repo.discard_file(&expected),
        Err(Error::Rule(RuleError::StaleFile))
    ));
    assert_eq!(fs::read(&selected).unwrap(), b"external edit\n");
    let expected = repo.prepare_discard(&file).unwrap();
    repo.discard_file(&expected).unwrap();
    assert_eq!(fs::read(&selected).unwrap(), b"staged\n");
    assert_eq!(
        fs::read(repo.root().join("other.txt")).unwrap(),
        b"unrelated\n"
    );
    assert_eq!(repo.status().unwrap().staged.len(), 1);
}

#[test]
fn discarding_a_staged_rename_restores_both_paths_and_keeps_other_changes() {
    let (_directory, repo) = fixture();
    commit_files(
        &repo,
        &[("before.txt", b"original\n"), ("other.txt", b"other\n")],
    );
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    fs::write(repo.root().join("after.txt"), b"unstaged too\n").unwrap();
    fs::write(repo.root().join("other.txt"), b"keep this\n").unwrap();
    let file = target(&repo, Scope::Index, "after.txt");
    assert_eq!(file.change.code, b'R');
    repo.discard_file(&repo.prepare_discard(&file).unwrap())
        .unwrap();
    assert_eq!(
        fs::read(repo.root().join("before.txt")).unwrap(),
        b"original\n"
    );
    assert!(!repo.root().join("after.txt").exists());
    assert_eq!(
        fs::read(repo.root().join("other.txt")).unwrap(),
        b"keep this\n"
    );
}

#[test]
fn discard_handles_untracked_files_and_staged_additions_without_a_head() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("discard.txt"), b"new\n").unwrap();
    fs::write(repo.root().join("keep.txt"), b"keep\n").unwrap();
    let file = target(&repo, Scope::Worktree, "discard.txt");
    repo.discard_file(&repo.prepare_discard(&file).unwrap())
        .unwrap();
    assert!(!repo.root().join("discard.txt").exists());
    fs::write(repo.root().join("discard.txt"), b"new staged\n").unwrap();
    repo.stage(Some(&file.change)).unwrap();
    let file = target(&repo, Scope::Index, "discard.txt");
    repo.discard_file(&repo.prepare_discard(&file).unwrap())
        .unwrap();
    assert!(!repo.root().join("discard.txt").exists());
    assert_eq!(fs::read(repo.root().join("keep.txt")).unwrap(), b"keep\n");
    assert!(repo.status().unwrap().staged.is_empty());
}

#[test]
fn ignore_rules_escape_literal_names_preserve_newlines_and_are_idempotent() {
    let (_directory, repo) = fixture();
    let path = "file [1] #!.txt";
    fs::write(repo.root().join(path), b"ignored\n").unwrap();
    fs::write(repo.root().join(".gitignore"), b"# existing\r\nold-rule").unwrap();
    let change = target(&repo, Scope::Worktree, path).change;
    repo.ignore_file(&change, IgnoreRule::File).unwrap();
    let once = fs::read(repo.root().join(".gitignore")).unwrap();
    repo.ignore_file(&change, IgnoreRule::File).unwrap();
    assert_eq!(fs::read(repo.root().join(".gitignore")).unwrap(), once);
    assert!(once.starts_with(b"# existing\r\nold-rule\r\n"));
    assert!(once.ends_with(b"\r\n"));
    git_output(command(Some(repo.root())).args(["check-ignore", "--", path]));
    assert_eq!(
        command(Some(repo.root()))
            .args(["check-ignore", "--", "file 1 #!.txt"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    assert!(!repo.root().join(".gitignore.lock").exists());
}

#[test]
fn extension_ignore_matches_nested_files_but_keeps_tracked_files_until_requested() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("tracked.log", b"tracked\n")]);
    fs::create_dir(repo.root().join("nested")).unwrap();
    fs::write(repo.root().join("nested/another.log"), b"untracked\n").unwrap();
    fs::write(repo.root().join("tracked.log"), b"working\n").unwrap();
    let change = target(&repo, Scope::Worktree, "tracked.log").change;
    repo.ignore_file(&change, IgnoreRule::Extension).unwrap();
    assert_eq!(
        repo.status()
            .unwrap()
            .unstaged
            .iter()
            .filter(|change| change.path == Path::new("tracked.log"))
            .count(),
        1
    );
    git_output(command(Some(repo.root())).args(["check-ignore", "--", "nested/another.log"]));
    repo.stop_tracking(&change).unwrap();
    assert_eq!(
        fs::read(repo.root().join("tracked.log")).unwrap(),
        b"working\n"
    );
    assert_eq!(target(&repo, Scope::Index, "tracked.log").change.code, b'D');
}

#[test]
fn ignore_refuses_invalid_paths_and_respects_an_existing_lock() {
    let (_directory, repo) = fixture();
    let invalid = Change {
        path: "../outside.txt".into(),
        previous: None,
        code: b'?',
    };
    assert!(matches!(
        repo.ignore_file(&invalid, IgnoreRule::File),
        Err(Error::Rule(RuleError::InvalidFilePath))
    ));
    let change = Change {
        path: "file.txt".into(),
        previous: None,
        code: b'?',
    };
    fs::write(repo.root().join(".gitignore.lock"), b"another writer").unwrap();
    assert!(repo.ignore_file(&change, IgnoreRule::File).is_err());
    assert_eq!(
        fs::read(repo.root().join(".gitignore.lock")).unwrap(),
        b"another writer"
    );
    assert!(!repo.root().join(".gitignore").exists());
}

#[test]
fn stash_file_preserves_other_staged_unstaged_and_untracked_changes() {
    let (_directory, repo) = fixture();
    commit_files(
        &repo,
        &[("selected.txt", b"base\n"), ("other.txt", b"other\n")],
    );
    fs::write(repo.root().join("selected.txt"), b"staged\n").unwrap();
    fs::write(repo.root().join("other.txt"), b"other staged\n").unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("selected.txt"), b"working\n").unwrap();
    fs::write(repo.root().join("other.txt"), b"other working\n").unwrap();
    fs::write(repo.root().join("new.txt"), b"new\n").unwrap();
    let change = target(&repo, Scope::Worktree, "selected.txt").change;
    repo.stash_file(&change).unwrap();
    assert_eq!(
        fs::read(repo.root().join("selected.txt")).unwrap(),
        b"base\n"
    );
    assert_eq!(
        fs::read(repo.root().join("other.txt")).unwrap(),
        b"other working\n"
    );
    assert_eq!(fs::read(repo.root().join("new.txt")).unwrap(), b"new\n");
    let status = repo.status().unwrap();
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, Path::new("other.txt"));
    let staged = FileTarget {
        scope: Scope::Index,
        change: status.staged[0].clone(),
    };
    assert_eq!(repo.file_contents(&staged).unwrap(), "other staged\n");
    repo.pop_stash().unwrap();
    assert_eq!(
        fs::read(repo.root().join("selected.txt")).unwrap(),
        b"working\n"
    );
    assert_eq!(
        fs::read(repo.root().join("other.txt")).unwrap(),
        b"other working\n"
    );
    assert_eq!(
        repo.file_contents(&target(&repo, Scope::Index, "selected.txt"))
            .unwrap(),
        "staged\n"
    );
    let change = target(&repo, Scope::Worktree, "new.txt").change;
    repo.stash_file(&change).unwrap();
    assert!(!repo.root().join("new.txt").exists());
    assert_eq!(
        fs::read(repo.root().join("other.txt")).unwrap(),
        b"other working\n"
    );
    repo.pop_stash().unwrap();
    assert_eq!(fs::read(repo.root().join("new.txt")).unwrap(), b"new\n");
}

#[test]
fn binary_patch_export_preserves_the_selected_index_version() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("data [1].bin", &[0, 1, 2, 255])]);
    fs::write(repo.root().join("data [1].bin"), [0, 3, 4, 254]).unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("data [1].bin"), [0, 9, 9, 254]).unwrap();
    let file = target(&repo, Scope::Index, "data [1].bin");
    let patch = repo.export_patch(&file).unwrap();
    assert!(String::from_utf8_lossy(&patch).contains("GIT binary patch"));
    repo.unstage(Some(&file.change)).unwrap();
    fs::write(repo.root().join("data [1].bin"), [0, 1, 2, 255]).unwrap();
    git_output(command(Some(repo.root())).args(["update-index", "--refresh"]));
    fs::write(repo.root().join("change.patch"), patch).unwrap();
    git_output(command(Some(repo.root())).args(["apply", "--index", "--", "change.patch"]));
    assert_eq!(
        fs::read(repo.root().join("data [1].bin")).unwrap(),
        [0, 3, 4, 254]
    );
}

#[test]
fn untracked_text_patch_can_be_applied_without_staging_the_original_file() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("baseline.txt", b"baseline\n")]);
    let name = "空格 [sample].txt";
    fs::write(repo.root().join(name), b"first\r\nlast").unwrap();
    let file = target(&repo, Scope::Worktree, name);
    let patch = repo.export_patch(&file).unwrap();
    assert!(repo.status().unwrap().staged.is_empty());
    fs::remove_file(repo.root().join(name)).unwrap();
    fs::write(repo.root().join("change.patch"), patch).unwrap();
    git_output(command(Some(repo.root())).args(["apply", "--", "change.patch"]));
    assert_eq!(fs::read(repo.root().join(name)).unwrap(), b"first\r\nlast");
}

#[test]
fn file_history_follows_renames_and_returns_paths_that_can_be_previewed() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("before.txt", b"original\n")]);
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    repo.commit("Rename", "").unwrap();
    fs::write(repo.root().join("after.txt"), b"modified\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Modify", "").unwrap();
    let change = Change {
        path: "after.txt".into(),
        previous: None,
        code: b'M',
    };
    let limited = repo.file_history(&change, 2).unwrap();
    assert!(limited.has_more);
    assert_eq!(limited.commits.len(), 2);
    let history = repo.file_history(&change, 10).unwrap();
    assert!(!history.has_more);
    assert_eq!(history.commits.len(), 3);
    assert_eq!(history.commits[0].commit.subject, "Modify");
    assert_eq!(
        history.commits[1].change.previous.as_deref(),
        Some(Path::new("before.txt"))
    );
    assert_eq!(history.commits[2].change.path, Path::new("before.txt"));
    for revision in history.commits {
        let file = FileTarget {
            scope: Scope::Commit {
                id: revision.commit.id,
                parent: revision.commit.parents.first().cloned(),
            },
            change: revision.change,
        };
        assert!(!repo.patch(&file).unwrap().bytes().is_empty());
    }
}

#[test]
fn blame_uses_the_selected_index_or_working_version() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("source.txt", b"one\ntwo\n")]);
    fs::write(repo.root().join("source.txt"), b"one\nstaged\n").unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("source.txt"), b"working\ntwo\n").unwrap();
    let staged = repo
        .blame_file(&target(&repo, Scope::Index, "source.txt"))
        .unwrap();
    let working = repo
        .blame_file(&target(&repo, Scope::Worktree, "source.txt"))
        .unwrap();
    assert_eq!(staged.len(), 2);
    assert_eq!(staged[0].author, "RitKraken Test");
    assert!(staged[1].commit.bytes().all(|byte| byte == b'0'));
    assert_eq!(staged[1].text, "staged");
    assert_eq!(working.len(), 2);
    assert!(working[0].commit.bytes().all(|byte| byte == b'0'));
    assert_eq!(working[1].author, "RitKraken Test");
    assert_eq!(working[1].number, 2);
    assert_eq!(working[1].text, "two");
}

#[test]
fn history_and_blame_resolve_an_uncommitted_rename_from_either_file_list() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("before.txt", b"original\nsecond\n")]);
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    fs::write(repo.root().join("after.txt"), b"original\nworking\n").unwrap();
    for scope in [Scope::Index, Scope::Worktree] {
        let file = target(&repo, scope.clone(), "after.txt");
        let history = repo.file_history(&file.change, 10).unwrap();
        assert_eq!(history.commits.len(), 1);
        assert_eq!(history.commits[0].change.path, Path::new("before.txt"));
        let blame = repo.blame_file(&file).unwrap();
        assert_eq!(blame[0].author, "RitKraken Test");
        assert_eq!(blame[0].text, "original");
        assert_eq!(
            blame[1].text,
            if scope == Scope::Index {
                "second"
            } else {
                "working"
            }
        );
    }
}

#[test]
fn stashing_a_rename_preserves_the_other_file_and_can_be_restored() {
    let (_directory, repo) = fixture();
    commit_files(
        &repo,
        &[("before.txt", b"original\n"), ("other.txt", b"other\n")],
    );
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    fs::write(repo.root().join("other.txt"), b"keep\n").unwrap();
    let file = target(&repo, Scope::Index, "after.txt");
    repo.stash_file(&file.change).unwrap();
    assert_eq!(
        fs::read(repo.root().join("before.txt")).unwrap(),
        b"original\n"
    );
    assert!(!repo.root().join("after.txt").exists());
    assert_eq!(fs::read(repo.root().join("other.txt")).unwrap(), b"keep\n");
    repo.pop_stash().unwrap();
    assert_eq!(
        fs::read(repo.root().join("after.txt")).unwrap(),
        b"original\n"
    );
    assert!(!repo.root().join("before.txt").exists());
    assert_eq!(fs::read(repo.root().join("other.txt")).unwrap(), b"keep\n");
}

#[test]
fn discarding_a_rename_refuses_to_overwrite_a_recreated_old_path() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("before.txt", b"original\n")]);
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    let file = target(&repo, Scope::Index, "after.txt");
    let expected = repo.prepare_discard(&file).unwrap();
    fs::write(repo.root().join("before.txt"), b"external new file\n").unwrap();
    assert!(matches!(
        repo.discard_file(&expected),
        Err(Error::Rule(RuleError::StaleFile))
    ));
    assert_eq!(
        fs::read(repo.root().join("before.txt")).unwrap(),
        b"external new file\n"
    );
    assert_eq!(
        fs::read(repo.root().join("after.txt")).unwrap(),
        b"original\n"
    );
}

#[test]
fn stop_tracking_a_rename_only_removes_the_current_index_entry() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("before.txt", b"original\n")]);
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    let change = target(&repo, Scope::Index, "after.txt").change;
    repo.ignore_file(&change, IgnoreRule::File).unwrap();
    repo.stop_tracking(&change).unwrap();
    assert_eq!(
        fs::read(repo.root().join("after.txt")).unwrap(),
        b"original\n"
    );
    assert_eq!(target(&repo, Scope::Index, "before.txt").change.code, b'D');
    git_output(command(Some(repo.root())).args(["check-ignore", "--", "after.txt"]));
}

#[test]
fn a_conflicting_file_stash_restore_keeps_the_stash_and_newer_edits() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("selected.txt", b"base\n")]);
    fs::write(repo.root().join("selected.txt"), b"staged\n").unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("selected.txt"), b"working\n").unwrap();
    repo.stash_file(&target(&repo, Scope::Index, "selected.txt").change)
        .unwrap();
    let before = git_output(command(Some(repo.root())).args(["rev-parse", "refs/stash"]));
    fs::write(repo.root().join("selected.txt"), b"newer edit\n").unwrap();
    assert!(repo.pop_stash().is_err());
    assert_eq!(
        fs::read(repo.root().join("selected.txt")).unwrap(),
        b"newer edit\n"
    );
    assert_eq!(
        git_output(command(Some(repo.root())).args(["rev-parse", "refs/stash"])),
        before
    );
    assert!(repo.status().unwrap().staged.is_empty());
    fs::write(repo.root().join("selected.txt"), b"base\n").unwrap();
    repo.pop_stash().unwrap();
    assert_eq!(
        fs::read(repo.root().join("selected.txt")).unwrap(),
        b"working\n"
    );
    assert_eq!(
        repo.file_contents(&target(&repo, Scope::Index, "selected.txt"))
            .unwrap(),
        "staged\n"
    );
}

#[test]
fn historical_file_history_and_blame_use_the_selected_commit_across_a_rename() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("before.txt", b"original\nsecond\n")]);
    let initial = head(&repo);
    git_output(command(Some(repo.root())).args(["mv", "--", "before.txt", "after.txt"]));
    repo.commit("Rename", "").unwrap();
    let renamed = head(&repo);
    fs::write(repo.root().join("after.txt"), b"future commit\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Future", "").unwrap();
    fs::write(repo.root().join("after.txt"), b"current working tree\n").unwrap();
    let file = FileTarget {
        scope: Scope::Commit {
            id: renamed,
            parent: Some(initial),
        },
        change: Change {
            path: "after.txt".into(),
            previous: Some("before.txt".into()),
            code: b'R',
        },
    };
    let history = repo.file_history_at(&file, 20).unwrap();
    assert_eq!(history.commits.len(), 2);
    assert_eq!(history.commits[0].commit.subject, "Rename");
    assert_eq!(history.commits[1].change.path, Path::new("before.txt"));
    let blame = repo.blame_file(&file).unwrap();
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].text, "original");
    assert_eq!(blame[1].text, "second");
    assert!(blame.iter().all(|line| line.author == "RitKraken Test"));
    let working = target(&repo, Scope::Worktree, "after.txt");
    assert_eq!(
        repo.file_history(&working.change, 20)
            .unwrap()
            .commits
            .len(),
        3
    );
}

#[test]
fn historical_file_export_is_binary_safe_read_only_and_cleans_up() {
    let (_directory, repo) = fixture();
    fs::create_dir(repo.root().join("assets")).unwrap();
    let name = "assets/旧版本 [1].bin";
    commit_files(&repo, &[(name, &[0, 1, 2, 255])]);
    let initial = head(&repo);
    fs::write(repo.root().join(name), [0, 8, 9, 255]).unwrap();
    repo.stage(None).unwrap();
    repo.commit("New version", "").unwrap();
    fs::remove_file(repo.root().join(name)).unwrap();
    let before = git_output(command(Some(repo.root())).args(["status", "--porcelain=v1", "-z"]));
    let snapshot = repo.export_commit_file(&initial, Path::new(name)).unwrap();
    let path = snapshot.path().to_owned();
    assert_eq!(path.file_name(), Path::new(name).file_name());
    assert_eq!(fs::read(&path).unwrap(), [0, 1, 2, 255]);
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    assert!(!path.starts_with(repo.root()));
    assert!(!repo.root().join(name).exists());
    assert_eq!(
        git_output(command(Some(repo.root())).args(["status", "--porcelain=v1", "-z"])),
        before
    );
    drop(snapshot);
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn deleted_historical_files_keep_their_history_and_deletion_patch() {
    let (_directory, repo) = fixture();
    commit_files(&repo, &[("removed.txt", b"removed content\n")]);
    let parent = head(&repo);
    fs::remove_file(repo.root().join("removed.txt")).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Remove file", "").unwrap();
    let file = FileTarget {
        scope: Scope::Commit {
            id: head(&repo),
            parent: Some(parent),
        },
        change: Change {
            path: "removed.txt".into(),
            previous: None,
            code: b'D',
        },
    };
    let history = repo.file_history_at(&file, 20).unwrap();
    assert_eq!(history.commits.len(), 2);
    assert_eq!(history.commits[0].change.code, b'D');
    let patch = repo.export_patch(&file).unwrap();
    assert!(String::from_utf8_lossy(&patch).contains("-removed content"));
    assert!(matches!(
        repo.blame_file(&file),
        Err(Error::Rule(RuleError::DeletedFile))
    ));
}
