mod support;

use std::fs;

use ritkraken_git::{FileTarget, Scope};
use support::fixture;

#[test]
fn unrelated_changes_update_status_without_changing_the_preview_version() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("preview.txt"), "original\n").unwrap();
    fs::write(repo.root().join("other.txt"), "other\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial", "").unwrap();
    fs::write(repo.root().join("preview.txt"), "preview edit\n").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot, repo.snapshot(None).unwrap());
    let target = FileTarget {
        scope: Scope::Worktree,
        change: snapshot.status.unstaged[0].clone(),
    };
    let patch = repo.patch(&target).unwrap();
    fs::write(repo.root().join("other.txt"), "unrelated edit\n").unwrap();
    assert_ne!(snapshot, repo.snapshot(None).unwrap());
    assert_eq!(patch, repo.patch(&target).unwrap());
}

#[test]
fn preview_changes_are_detected_even_when_the_snapshot_is_identical() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("preview.txt"), "original\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial", "").unwrap();
    fs::write(repo.root().join("preview.txt"), "first edit\n").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    let target = FileTarget {
        scope: Scope::Worktree,
        change: snapshot.status.unstaged[0].clone(),
    };
    let patch = repo.patch(&target).unwrap();
    fs::write(repo.root().join("preview.txt"), "second edit\n").unwrap();
    assert_eq!(snapshot, repo.snapshot(None).unwrap());
    assert_ne!(patch, repo.patch(&target).unwrap());
    assert_eq!(repo.file_contents(&target).unwrap(), "second edit\n");
}

#[test]
fn index_preview_depends_on_the_index_instead_of_worktree_edits() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("preview.txt"), "original\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial", "").unwrap();
    fs::write(repo.root().join("preview.txt"), "staged text\n").unwrap();
    repo.stage(None).unwrap();
    let target = FileTarget {
        scope: Scope::Index,
        change: repo.status().unwrap().staged[0].clone(),
    };
    let patch = repo.patch(&target).unwrap();
    fs::write(repo.root().join("preview.txt"), "unstaged text\n").unwrap();
    assert_eq!(patch, repo.patch(&target).unwrap());
    assert_eq!(repo.file_contents(&target).unwrap(), "staged text\n");
    repo.stage(None).unwrap();
    assert_ne!(patch, repo.patch(&target).unwrap());
    assert_eq!(repo.file_contents(&target).unwrap(), "unstaged text\n");
}

#[test]
fn committed_preview_is_immutable_across_new_commits() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("preview.txt"), "committed text\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial", "").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    let target = FileTarget {
        scope: Scope::Commit {
            id: snapshot.head.unwrap(),
            parent: None,
        },
        change: ritkraken_git::Change {
            path: "preview.txt".into(),
            previous: None,
            code: b'A',
        },
    };
    let patch = repo.patch(&target).unwrap();
    fs::write(repo.root().join("preview.txt"), "new committed text\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Second", "").unwrap();
    assert_eq!(patch, repo.patch(&target).unwrap());
    assert_eq!(repo.file_contents(&target).unwrap(), "committed text\n");
}

#[test]
fn presentation_options_are_part_of_the_preview_version() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("preview.txt"), "original\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial", "").unwrap();
    fs::write(repo.root().join("preview.txt"), "edited\n").unwrap();
    let target = FileTarget {
        scope: Scope::Worktree,
        change: repo.status().unwrap().unstaged[0].clone(),
    };
    let ordinary = repo.patch(&target).unwrap();
    let ignored = repo
        .patch(&target)
        .unwrap()
        .with_edge_whitespace_ignored(true);
    assert_eq!(ordinary.bytes(), ignored.bytes());
    assert_ne!(ordinary, ignored);
}
