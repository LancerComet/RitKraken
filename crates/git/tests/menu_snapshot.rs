mod support;

use ritkraken_git::{
    CommitAction, CommitAvailability, CommitBlock, CommitRelation, Repo, Snapshot,
};
use std::{fs, sync::Arc};
use support::{commits::*, fixture};

const ACTIONS: [CommitAction; 14] = [
    CommitAction::CreateBranch,
    CommitAction::CreateTag,
    CommitAction::Switch,
    CommitAction::Checkout,
    CommitAction::CherryPick,
    CommitAction::Revert,
    CommitAction::Merge,
    CommitAction::Rebase,
    CommitAction::InteractiveRebase,
    CommitAction::Amend,
    CommitAction::Reset,
    CommitAction::Continue,
    CommitAction::Skip,
    CommitAction::Abort,
];

fn agrees_with_git(repo: &Repo, snapshot: &Snapshot, id: &str) {
    let menu = repo.commit_menu_snapshot(snapshot, None).unwrap();
    let commit = snapshot
        .commits
        .iter()
        .find(|commit| commit.id == id)
        .unwrap();
    let fast = menu.state(snapshot, commit).unwrap();
    let live = repo.commit_context(id).unwrap();
    assert_eq!(fast.relation, live.relation);
    assert_eq!(fast.linear_rebase, live.linear_rebase);
    assert_eq!(fast.references, live.references);
    assert_eq!(fast.tracked_remote, live.tracked_remote);
    assert_eq!(fast.remote_url, live.remote_url);
    for action in ACTIONS {
        assert_eq!(
            fast.availability(action),
            live.availability(action),
            "{action:?}"
        );
    }
}

#[test]
fn limited_history_classifies_descendants_even_when_head_is_not_in_the_list() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "ahead"]);
    commit_file(&repo, "one", "one", "One");
    let tip = commit_file(&repo, "two", "two", "Two");
    git(&repo, &["switch", "main"]);
    let snapshot = repo.snapshot(Some(1)).unwrap();
    assert_eq!(snapshot.commits[0].id, tip);
    assert!(
        snapshot
            .commits
            .iter()
            .all(|commit| Some(&commit.id) != snapshot.head.as_ref())
    );
    let menu = repo.commit_menu_snapshot(&snapshot, None).unwrap();
    assert_eq!(
        menu.state(&snapshot, &snapshot.commits[0])
            .unwrap()
            .relation,
        CommitRelation::Descendant
    );
    agrees_with_git(&repo, &snapshot, &tip);
}

#[test]
fn divergence_is_correct_when_the_common_ancestor_is_beyond_the_loaded_history() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let feature = commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    commit_file(&repo, "main", "main", "Main");
    let snapshot = repo.snapshot(Some(2)).unwrap();
    assert!(snapshot.commits.iter().all(|commit| commit.id != base));
    let menu = repo.commit_menu_snapshot(&snapshot, None).unwrap();
    let commit = snapshot
        .commits
        .iter()
        .find(|commit| commit.id == feature)
        .unwrap();
    assert_eq!(
        menu.state(&snapshot, commit).unwrap().relation,
        CommitRelation::Diverged
    );
    agrees_with_git(&repo, &snapshot, &feature);
}

#[test]
fn merge_boundaries_and_linear_rebase_bases_match_live_git_rules() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "diverged-before-merge"]);
    let before_merge = commit_file(&repo, "before", "before", "Before merge");
    git(&repo, &["switch", "-c", "feature", &base]);
    commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    commit_file(&repo, "main", "main", "Main");
    git(&repo, &["merge", "--no-edit", "feature"]);
    let merge = head(&repo);
    let linear = commit_file(&repo, "linear", "linear", "Linear");
    commit_file(&repo, "tip", "tip", "Tip");
    git(&repo, &["switch", "-c", "diverged-after-merge", &merge]);
    let after_merge = commit_file(&repo, "after", "after", "After merge");
    git(&repo, &["switch", "main"]);
    let snapshot = repo.snapshot(None).unwrap();
    for id in [base, merge, linear, before_merge, after_merge] {
        agrees_with_git(&repo, &snapshot, &id);
    }
}

#[test]
fn worktree_only_refresh_reuses_relationships_and_menu_construction_uses_snapshot_facts() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a", "base", "Base");
    let clean = repo.snapshot(None).unwrap();
    let menu = repo.commit_menu_snapshot(&clean, None).unwrap();
    fs::write(repo.root().join("a"), "modified externally").unwrap();
    // Disk changes cannot affect a pure snapshot menu until its snapshot refreshes.
    assert!(!menu.state(&clean, &clean.commits[0]).unwrap().dirty);
    let dirty = repo.snapshot(None).unwrap();
    let refreshed = repo.commit_menu_snapshot(&dirty, Some(&menu)).unwrap();
    assert!(Arc::ptr_eq(&menu, &refreshed));
    let state = refreshed.state(&dirty, &dirty.commits[0]).unwrap();
    assert_eq!(
        state.availability(CommitAction::Revert),
        CommitAvailability::Disabled(CommitBlock::Dirty)
    );
    assert_eq!(
        state.availability(CommitAction::Amend),
        CommitAvailability::Enabled
    );
    commit_file(&repo, "a", "new commit", "New commit");
    let changed = repo.snapshot(None).unwrap();
    assert!(!Arc::ptr_eq(
        &refreshed,
        &repo
            .commit_menu_snapshot(&changed, Some(&refreshed))
            .unwrap()
    ));
}

#[test]
fn replace_refs_invalidate_cached_relationships() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let feature = commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    commit_file(&repo, "main", "main", "Main");
    git(&repo, &["switch", "--orphan", "orphan"]);
    let orphan = commit_file(&repo, "other", "other", "Other root");
    git(&repo, &["switch", "main"]);
    let before = repo.snapshot(None).unwrap();
    let menu = repo.commit_menu_snapshot(&before, None).unwrap();
    let target = before
        .commits
        .iter()
        .find(|commit| commit.id == orphan)
        .unwrap();
    assert_eq!(
        menu.state(&before, target).unwrap().relation,
        CommitRelation::Unrelated
    );
    git(&repo, &["replace", &orphan, &feature]);
    let after = repo.snapshot(None).unwrap();
    let refreshed = repo.commit_menu_snapshot(&after, Some(&menu)).unwrap();
    assert!(!Arc::ptr_eq(&menu, &refreshed));
    agrees_with_git(&repo, &after, &orphan);
}

#[test]
fn shallow_boundaries_invalidate_the_cache_without_a_changed_head_or_visible_ids() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a", "base", "Base");
    let tip = commit_file(&repo, "a", "tip", "Tip");
    let before = repo.snapshot(Some(1)).unwrap();
    let menu = repo.commit_menu_snapshot(&before, None).unwrap();
    fs::write(repo.root().join(".git").join("shallow"), format!("{tip}\n")).unwrap();
    let after = repo.snapshot(Some(1)).unwrap();
    assert_eq!(before.head, after.head);
    assert_eq!(before.commits[0].id, after.commits[0].id);
    assert!(!Arc::ptr_eq(
        &menu,
        &repo.commit_menu_snapshot(&after, Some(&menu)).unwrap()
    ));
}

#[test]
fn snapshot_remote_urls_follow_git_url_rewriting_and_prefer_origin() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a", "base", "Base");
    git(
        &repo,
        &[
            "remote",
            "add",
            "another",
            "https://gitlab.com/group/repository.git",
        ],
    );
    git(
        &repo,
        &["config", "url.https://github.com/.insteadOf", "gh:"],
    );
    git(
        &repo,
        &["remote", "add", "origin", "gh:group/repository.git"],
    );
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot.remotes, vec!["another", "origin"]);
    let menu = repo.commit_menu_snapshot(&snapshot, None).unwrap();
    let state = menu.state(&snapshot, &snapshot.commits[0]).unwrap();
    assert_eq!(
        state.remote_url.as_deref(),
        Some(git(&repo, &["remote", "get-url", "origin"]).trim())
    );
    assert_eq!(
        state.remote_url.as_deref(),
        Some("https://github.com/group/repository.git")
    );
}
