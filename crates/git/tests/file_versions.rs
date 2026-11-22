mod support;

use std::fs;

use ritkraken_git::{Change, FileTarget, Repo, Scope};
use support::commits::head;
use support::{command, fixture, git_output};

fn commit_bytes(repo: &Repo, path: &str, bytes: &[u8], message: &str) -> String {
    fs::write(repo.root().join(path), bytes).unwrap();
    git_output(command(Some(repo.root())).args(["add", "--", path]));
    repo.commit(message, "").unwrap();
    head(repo)
}

fn change(repo: &Repo, path: &str, staged: bool) -> Change {
    let status = repo.status().unwrap();
    let changes = if staged {
        &status.staged
    } else {
        &status.unstaged
    };
    changes
        .iter()
        .find(|change| change.path == std::path::Path::new(path))
        .unwrap_or_else(|| panic!("no change for {path}"))
        .clone()
}

fn target(scope: Scope, change: Change) -> FileTarget {
    FileTarget { scope, change }
}

#[test]
fn worktree_versions_read_the_index_and_disk_bytes() {
    let (_directory, repo) = fixture();
    commit_bytes(&repo, "image.png", b"\x89PNG\x00old", "Initial");
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00disk").unwrap();
    let file = target(Scope::Worktree, change(&repo, "image.png", false));
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old.as_deref(), Some(b"\x89PNG\x00old".as_slice()));
    assert_eq!(versions.new.as_deref(), Some(b"\x89PNG\x00disk".as_slice()));
}

#[test]
fn worktree_old_side_is_the_index_not_head() {
    let (_directory, repo) = fixture();
    commit_bytes(&repo, "image.png", b"\x89PNG\x00head", "Initial");
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00staged").unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00disk").unwrap();
    let file = target(Scope::Worktree, change(&repo, "image.png", false));
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(
        versions.old.as_deref(),
        Some(b"\x89PNG\x00staged".as_slice())
    );
    assert_eq!(versions.new.as_deref(), Some(b"\x89PNG\x00disk".as_slice()));
}

#[test]
fn index_versions_read_head_and_the_index() {
    let (_directory, repo) = fixture();
    commit_bytes(&repo, "image.png", b"\x89PNG\x00head", "Initial");
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00staged").unwrap();
    repo.stage(None).unwrap();
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00disk").unwrap();
    let file = target(Scope::Index, change(&repo, "image.png", true));
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old.as_deref(), Some(b"\x89PNG\x00head".as_slice()));
    assert_eq!(
        versions.new.as_deref(),
        Some(b"\x89PNG\x00staged".as_slice())
    );
}

#[test]
fn commit_versions_read_the_parent_and_commit_blobs() {
    let (_directory, repo) = fixture();
    let parent = commit_bytes(&repo, "image.png", b"\x89PNG\x00parent", "Initial");
    let id = commit_bytes(&repo, "image.png", b"\x89PNG\x00commit", "Update");
    let file = target(
        Scope::Commit {
            id,
            parent: Some(parent),
        },
        Change {
            path: "image.png".into(),
            previous: None,
            code: b'M',
        },
    );
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(
        versions.old.as_deref(),
        Some(b"\x89PNG\x00parent".as_slice())
    );
    assert_eq!(
        versions.new.as_deref(),
        Some(b"\x89PNG\x00commit".as_slice())
    );
}

#[test]
fn a_root_commit_has_no_old_side() {
    let (_directory, repo) = fixture();
    let id = commit_bytes(&repo, "image.png", b"\x89PNG\x00root", "Initial");
    let file = target(
        Scope::Commit { id, parent: None },
        Change {
            path: "image.png".into(),
            previous: None,
            code: b'M',
        },
    );
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old, None);
    assert_eq!(versions.new.as_deref(), Some(b"\x89PNG\x00root".as_slice()));
}

#[test]
fn added_and_untracked_files_have_no_old_side() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("added.png"), b"\x89PNG\x00added").unwrap();
    repo.stage(None).unwrap();
    let file = target(Scope::Index, change(&repo, "added.png", true));
    assert_eq!(file.change.code, b'A');
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old, None);
    assert_eq!(
        versions.new.as_deref(),
        Some(b"\x89PNG\x00added".as_slice())
    );

    fs::write(repo.root().join("untracked.png"), b"\x89PNG\x00new").unwrap();
    let file = target(Scope::Worktree, change(&repo, "untracked.png", false));
    assert_eq!(file.change.code, b'?');
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old, None);
    assert_eq!(versions.new.as_deref(), Some(b"\x89PNG\x00new".as_slice()));
}

#[test]
fn deleted_files_have_no_new_side() {
    let (_directory, repo) = fixture();
    commit_bytes(&repo, "image.png", b"\x89PNG\x00gone", "Initial");
    fs::remove_file(repo.root().join("image.png")).unwrap();
    let file = target(Scope::Worktree, change(&repo, "image.png", false));
    assert_eq!(file.change.code, b'D');
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old.as_deref(), Some(b"\x89PNG\x00gone".as_slice()));
    assert_eq!(versions.new, None);
}

#[test]
fn renamed_files_read_the_old_side_from_the_previous_path() {
    let (_directory, repo) = fixture();
    commit_bytes(&repo, "old.png", b"\x89PNG\x00before", "Initial");
    git_output(command(Some(repo.root())).args(["mv", "old.png", "new.png"]));
    fs::write(repo.root().join("new.png"), b"\x89PNG\x00after").unwrap();
    repo.stage(None).unwrap();
    let file = target(
        Scope::Index,
        Change {
            path: "new.png".into(),
            previous: Some("old.png".into()),
            code: b'R',
        },
    );
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(
        versions.old.as_deref(),
        Some(b"\x89PNG\x00before".as_slice())
    );
    assert_eq!(
        versions.new.as_deref(),
        Some(b"\x89PNG\x00after".as_slice())
    );
}

#[test]
fn index_scope_on_an_unborn_head_reads_no_old_side() {
    let (_directory, repo) = fixture();
    fs::write(repo.root().join("image.png"), b"\x89PNG\x00staged").unwrap();
    repo.stage(None).unwrap();
    // An unborn HEAD reports no old side even when the status code asks for one.
    let file = target(
        Scope::Index,
        Change {
            path: "image.png".into(),
            previous: None,
            code: b'M',
        },
    );
    let versions = repo.file_versions(&file).unwrap();
    assert_eq!(versions.old, None);
    assert_eq!(
        versions.new.as_deref(),
        Some(b"\x89PNG\x00staged".as_slice())
    );
}
