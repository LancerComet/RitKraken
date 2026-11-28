mod support;

use std::fs;

use ritkraken_git::{FileTarget, Scope};
use support::{commits::*, fixture};

#[test]
fn selected_paths_stage_and_unstage_together_without_touching_other_changes() {
    let (_directory, repo) = fixture();
    for name in [
        "a.txt",
        "b.txt",
        "literal[1].txt",
        "literal1.txt",
        "removed.txt",
    ] {
        fs::write(repo.root().join(name), "base\n").unwrap();
    }
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    for name in [
        "a.txt",
        "b.txt",
        "literal[1].txt",
        "literal1.txt",
        "新しい file.txt",
    ] {
        fs::write(repo.root().join(name), "changed\n").unwrap();
    }
    fs::remove_file(repo.root().join("removed.txt")).unwrap();
    let paths = ["a.txt", "literal[1].txt", "新しい file.txt", "removed.txt"];
    let changes: Vec<_> = repo
        .status()
        .unwrap()
        .unstaged
        .into_iter()
        .filter(|change| {
            paths
                .iter()
                .any(|path| change.path == std::path::Path::new(path))
        })
        .collect();
    repo.stage_files(Some(&changes)).unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.staged.len(), 4);
    assert_eq!(status.unstaged.len(), 2);
    assert!(
        status
            .unstaged
            .iter()
            .any(|file| file.path == std::path::Path::new("literal1.txt"))
    );
    let worktree: Vec<_> = [
        "a.txt",
        "b.txt",
        "literal[1].txt",
        "literal1.txt",
        "新しい file.txt",
    ]
    .map(|name| fs::read(repo.root().join(name)).unwrap())
    .into();
    repo.unstage_files(Some(&status.staged[..2])).unwrap();
    assert_eq!(repo.status().unwrap().staged.len(), 2);
    repo.unstage_files(Some(&status.staged[2..])).unwrap();
    assert!(repo.status().unwrap().staged.is_empty());
    for (name, bytes) in [
        "a.txt",
        "b.txt",
        "literal[1].txt",
        "literal1.txt",
        "新しい file.txt",
    ]
    .into_iter()
    .zip(worktree)
    {
        assert_eq!(fs::read(repo.root().join(name)).unwrap(), bytes);
    }
    assert!(!repo.root().join("removed.txt").exists());
}

#[test]
fn unborn_index_and_empty_selection_do_not_change_unselected_paths() {
    let (_directory, repo) = fixture();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(repo.root().join(name), name).unwrap();
    }
    let changes = repo.status().unwrap().unstaged;
    repo.stage_files(Some(&[])).unwrap();
    assert!(repo.status().unwrap().staged.is_empty());
    repo.stage_files(Some(&changes)).unwrap();
    repo.unstage_files(Some(&[])).unwrap();
    assert_eq!(repo.status().unwrap().staged.len(), 3);
    repo.unstage_files(Some(&changes[..2])).unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.staged.len(), 1);
    assert_eq!(status.staged[0].path, changes[2].path);
    assert_eq!(status.unstaged.len(), 2);
    for change in changes {
        assert!(repo.root().join(change.path).is_file());
    }
}

#[test]
fn batch_paths_support_renames_and_selections_larger_than_a_windows_command_line() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "old.txt", "rename\n", "Base");
    git(&repo, &["mv", "old.txt", "新しい name.txt"]);
    let rename = repo.status().unwrap().staged;
    repo.unstage_files(Some(&rename)).unwrap();
    assert_eq!(git(&repo, &["show", ":old.txt"]), "rename\n");
    assert!(repo.root().join("新しい name.txt").exists());
    let changes = repo.status().unwrap().unstaged;
    repo.stage_files(Some(&changes)).unwrap();
    assert_eq!(git(&repo, &["show", ":新しい name.txt"]), "rename\n");
    for index in 0..350 {
        let name = format!("{index:04}-{}.txt", "long-file-name".repeat(9));
        fs::write(repo.root().join(name), "long selection\n").unwrap();
    }
    let selected = repo.status().unwrap().unstaged;
    assert!(
        selected
            .iter()
            .map(|file| file.path.as_os_str().len())
            .sum::<usize>()
            > 32767
    );
    repo.stage_files(Some(&selected)).unwrap();
    assert!(repo.status().unwrap().unstaged.is_empty());
    repo.unstage_files(Some(&selected)).unwrap();
    assert_eq!(repo.status().unwrap().unstaged.len(), 350);
    assert_eq!(repo.status().unwrap().staged.len(), 1);
}

#[test]
fn discard_checks_the_entire_selection_before_removing_any_changes() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a.txt", "base\n", "Base");
    commit_file(&repo, "b.txt", "base\n", "Second");
    for name in ["a.txt", "b.txt"] {
        fs::write(repo.root().join(name), "changed\n").unwrap();
    }
    let changes = repo.status().unwrap().unstaged;
    let files: Vec<_> = changes
        .into_iter()
        .map(|change| FileTarget {
            scope: Scope::Worktree,
            change,
        })
        .collect();
    let expected: Vec<_> = files
        .iter()
        .map(|file| repo.prepare_discard(file).unwrap())
        .collect();
    fs::write(repo.root().join("b.txt"), "external edit\n").unwrap();
    assert!(repo.discard_files(&expected).is_err());
    assert_eq!(
        fs::read_to_string(repo.root().join("a.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("b.txt")).unwrap(),
        "external edit\n"
    );
    let expected: Vec<_> = files
        .iter()
        .map(|file| repo.prepare_discard(file).unwrap())
        .collect();
    repo.discard_files(&expected).unwrap();
    assert!(repo.status().unwrap().is_empty());
}

#[test]
fn prepare_discards_and_discard_files_clear_all_unstaged_while_preserving_staged() {
    let (_directory, repo) = fixture();
    for (name, contents) in [
        ("modified.txt", "base\n"),
        ("both.txt", "base\n"),
        ("staged_only.txt", "base\n"),
    ] {
        fs::write(repo.root().join(name), contents).unwrap();
    }
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();

    fs::write(repo.root().join("modified.txt"), "unstaged edit\n").unwrap();
    fs::write(repo.root().join("both.txt"), "staged edit\n").unwrap();
    git(&repo, &["add", "--", "both.txt"]);
    fs::write(repo.root().join("both.txt"), "staged edit\nunstaged edit\n").unwrap();
    fs::write(repo.root().join("staged_only.txt"), "staged edit\n").unwrap();
    git(&repo, &["add", "--", "staged_only.txt"]);
    fs::write(repo.root().join("untracked.txt"), "untracked\n").unwrap();

    let unstaged_targets: Vec<_> = repo
        .status()
        .unwrap()
        .unstaged
        .into_iter()
        .map(|change| FileTarget {
            scope: Scope::Worktree,
            change,
        })
        .collect();
    assert_eq!(unstaged_targets.len(), 3);
    let prepared = repo.prepare_discards(&unstaged_targets).unwrap();
    repo.discard_files(&prepared).unwrap();

    let status = repo.status().unwrap();
    assert!(status.unstaged.is_empty());
    assert!(!repo.root().join("untracked.txt").exists());
    assert_eq!(
        fs::read_to_string(repo.root().join("modified.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("both.txt")).unwrap(),
        "staged edit\n"
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("staged_only.txt")).unwrap(),
        "staged edit\n"
    );
    assert_eq!(status.staged.len(), 2);
    assert_eq!(git(&repo, &["show", ":both.txt"]), "staged edit\n");
    assert_eq!(git(&repo, &["show", ":staged_only.txt"]), "staged edit\n");
}
