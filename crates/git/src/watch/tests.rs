use super::*;
use notify::{
    EventKind,
    event::{AccessKind, ModifyKind},
};

#[path = "../../tests/support/mod.rs"]
mod support;
use support::{command, fixture, git_output};

fn modified(path: PathBuf) -> Event {
    Event::new(EventKind::Modify(ModifyKind::Any)).add_path(path)
}

#[test]
fn only_checkout_and_relevant_git_metadata_invalidate_the_snapshot() {
    let (_directory, repo) = fixture();
    let paths = WatchPaths::new(&repo).unwrap();
    for path in [
        paths.git.join("HEAD"),
        paths.git.join("index"),
        paths.common.join("refs/heads/main"),
        paths.common.join("packed-refs"),
        paths.common.join("info/exclude"),
    ] {
        assert!(paths.changes(modified(path)).metadata);
    }
    for path in [
        paths.git.join("index.lock"),
        paths.common.join("objects/pack/cache.pack"),
        paths.common.join("logs/HEAD"),
    ] {
        assert!(paths.changes(modified(path)).is_empty());
    }
    let changes = paths.changes(modified(paths.root.join("source.rs")));
    assert_eq!(changes.paths, HashSet::from([PathBuf::from("source.rs")]));
    assert!(!changes.metadata);
    for path in [".gitignore", "source/.gitignore"] {
        assert!(paths.changes(modified(paths.root.join(path))).metadata);
    }
}

#[test]
fn git_reads_do_not_trigger_another_refresh() {
    let (_directory, repo) = fixture();
    let paths = WatchPaths::new(&repo).unwrap();
    let event = Event::new(EventKind::Access(AccessKind::Read)).add_path(paths.git.join("HEAD"));
    assert!(paths.changes(event).is_empty());
}

#[test]
fn rescan_flags_reinstall_the_watcher_even_on_access_events() {
    let (_directory, repo) = fixture();
    let paths = WatchPaths::new(&repo).unwrap();
    let event = Event::new(EventKind::Access(AccessKind::Read))
        .add_path(paths.root.clone())
        .set_flag(notify::event::Flag::Rescan);
    let changes = paths.changes(event);
    assert!(changes.metadata);
    assert!(changes.needs_restart());
}

#[test]
fn debounce_coalesces_a_burst_without_starving_continuous_edits() {
    let start = Instant::now();
    let change = || RepoChanges {
        paths: HashSet::from(["source.rs".into()]),
        ..Default::default()
    };
    let mut pending = Pending::default();
    pending.push(change(), start);
    pending.push(change(), start + Duration::from_millis(100));
    assert!(pending.take(start + Duration::from_millis(349)).is_none());
    assert_eq!(
        pending
            .take(start + Duration::from_millis(350))
            .unwrap()
            .paths
            .len(),
        1
    );
    assert!(pending.take(start + Duration::from_secs(2)).is_none());
    for index in 0..10 {
        pending.push(change(), start + Duration::from_millis(index * 100));
    }
    assert!(pending.take(start + MAX_DELAY).is_some());
}

#[test]
fn overflowing_the_path_budget_requests_a_bounded_full_refresh() {
    let mut pending = Pending::default();
    let paths = (0..=MAX_PATHS)
        .map(|index| PathBuf::from(index.to_string()))
        .collect();
    let now = Instant::now();
    pending.push(
        RepoChanges {
            paths,
            ..Default::default()
        },
        now,
    );
    let batch = pending.take(now + QUIET_PERIOD).unwrap();
    assert!(batch.metadata);
    assert!(batch.paths.is_empty());
}

#[test]
fn ignore_filter_preserves_tracked_files_and_handles_large_batches() {
    let (_directory, repo) = fixture();
    std::fs::create_dir(repo.root().join("out")).unwrap();
    std::fs::write(repo.root().join("out/tracked.txt"), "base\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    std::fs::write(repo.root().join(".gitignore"), "out/\n").unwrap();
    let ignored: HashSet<_> = (0..2000)
        .map(|index| PathBuf::from(format!("out/ignored-{index}.txt")))
        .collect();
    assert!(
        !RepoChanges {
            paths: ignored.clone(),
            ..Default::default()
        }
        .needs_refresh(&repo)
        .unwrap()
    );
    for path in ["out/tracked.txt", "new-file.txt", "旧文件.txt"] {
        let mut paths = ignored.clone();
        paths.insert(path.into());
        assert!(
            RepoChanges {
                paths,
                ..Default::default()
            }
            .needs_refresh(&repo)
            .unwrap()
        );
    }
}

#[test]
fn linked_worktree_watches_checkout_metadata_and_shared_refs() {
    let (_directory, repo) = fixture();
    std::fs::write(repo.root().join("source.rs"), "base\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    let parent = tempfile::tempdir().unwrap();
    let linked = parent.path().join("linked");
    git_output(
        command(Some(repo.root()))
            .args(["worktree", "add", "--detach"])
            .arg(&linked)
            .arg("HEAD"),
    );
    let repo = Repo::open(&linked).unwrap();
    let paths = WatchPaths::new(&repo).unwrap();
    assert!(!paths.git.starts_with(&paths.root));
    for path in [
        paths.git.join("HEAD"),
        paths.git.join("index"),
        paths.common.join("refs/heads/main"),
    ] {
        assert!(paths.changes(modified(path)).metadata);
    }
}

#[test]
fn removing_a_watched_root_requests_reinstallation() {
    let (_directory, repo) = fixture();
    let paths = WatchPaths::new(&repo).unwrap();
    let changes = paths.changes(
        Event::new(EventKind::Remove(notify::event::RemoveKind::Folder))
            .add_path(paths.git.clone()),
    );
    assert!(changes.metadata);
    assert!(changes.needs_restart());
}
