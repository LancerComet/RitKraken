mod support;

use ritkraken_git::{HistoryFilter, Repo};
use support::commits::{commit_file, git, head};
use support::fixture;

fn filtered(repo: &Repo, hidden: &[&str], solo: &[&str]) -> Vec<String> {
    let mut filter = HistoryFilter::default();
    for name in hidden {
        filter.hidden.insert((*name).to_owned());
    }
    for name in solo {
        filter.solo.insert((*name).to_owned());
    }
    repo.snapshot_filtered(None, &filter)
        .unwrap()
        .commits
        .iter()
        .map(|commit| commit.id.clone())
        .collect()
}

/// main <- base; `feature` forks from base, `other` also forks from base.
fn diverged(repo: &Repo) -> (String, String, String) {
    let base = commit_file(repo, "base.txt", "base\n", "Base");
    git(repo, &["checkout", "-b", "feature"]);
    let feature = commit_file(repo, "feature.txt", "feature\n", "Feature");
    git(repo, &["checkout", "main"]);
    git(repo, &["checkout", "-b", "other", &base]);
    let other = commit_file(repo, "other.txt", "other\n", "Other");
    git(repo, &["checkout", "main"]);
    commit_file(repo, "main.txt", "main\n", "Main");
    (base, feature, other)
}

#[test]
fn hiding_a_branch_removes_exclusive_commits_and_keeps_shared_ancestors() {
    let (_directory, repo) = fixture();
    let (base, feature, _other) = diverged(&repo);
    let ids = filtered(&repo, &["refs/heads/feature"], &[]);
    assert!(!ids.contains(&feature));
    assert!(ids.contains(&base), "shared ancestors stay: {ids:?}");
    assert!(ids.contains(&head(&repo)));
}

#[test]
fn hiding_the_checked_out_branch_still_shows_its_commits_via_head() {
    let (_directory, repo) = fixture();
    let (base, feature, _other) = diverged(&repo);
    let ids = filtered(&repo, &["refs/heads/main"], &[]);
    assert!(ids.contains(&base));
    assert!(ids.contains(&feature));
    assert!(ids.contains(&head(&repo)), "HEAD is always included");
}

#[test]
fn hiding_a_tag_removes_commits_only_reachable_through_it() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "base.txt", "base\n", "Base");
    git(&repo, &["checkout", "-b", "tmp"]);
    let tagged = commit_file(&repo, "tagged.txt", "tagged\n", "Tagged");
    git(&repo, &["tag", "v-tagged", &tagged]);
    git(&repo, &["checkout", "main"]);
    git(&repo, &["branch", "-D", "tmp"]);
    let ids = filtered(&repo, &[], &[]);
    assert!(ids.contains(&tagged));
    let ids = filtered(&repo, &["refs/tags/v-tagged"], &[]);
    assert!(!ids.contains(&tagged));
}

#[test]
fn solo_limits_the_graph_to_its_refs_plus_head() {
    let (_directory, repo) = fixture();
    let (base, feature, other) = diverged(&repo);
    let ids = filtered(&repo, &[], &["refs/heads/feature"]);
    assert!(ids.contains(&feature));
    assert!(ids.contains(&base));
    assert!(ids.contains(&head(&repo)), "HEAD is always included");
    assert!(!ids.contains(&other), "refs outside solo are excluded");
}

#[test]
fn solo_takes_precedence_over_hidden() {
    let (_directory, repo) = fixture();
    let (_base, feature, _other) = diverged(&repo);
    let ids = filtered(&repo, &["refs/heads/feature"], &["refs/heads/feature"]);
    assert!(ids.contains(&feature), "solo wins over hidden");
}

#[test]
fn a_deleted_solo_ref_is_ignored_without_error() {
    let (_directory, repo) = fixture();
    let (_base, feature, other) = diverged(&repo);
    let ids = filtered(&repo, &[], &["refs/heads/gone"]);
    assert!(ids.contains(&head(&repo)));
    assert!(!ids.contains(&feature));
    assert!(!ids.contains(&other));
}

#[test]
fn stash_commits_stay_listed_under_solo() {
    let (_directory, repo) = fixture();
    let (_base, _feature, _other) = diverged(&repo);
    std::fs::write(repo.root().join("stashed.txt"), "stash\n").unwrap();
    git(&repo, &["add", "--", "stashed.txt"]);
    git(&repo, &["stash", "push", "-m", "Stashed"]);
    let mut filter = HistoryFilter::default();
    filter.solo.insert("refs/heads/feature".to_owned());
    let snapshot = repo.snapshot_filtered(None, &filter).unwrap();
    assert_eq!(snapshot.stashes.len(), 1);
    let stash_id = &snapshot.stashes[0].id;
    assert!(
        snapshot.commits.iter().any(|commit| &commit.id == stash_id),
        "stash row stays listed"
    );
}

#[test]
fn snapshot_page_respects_the_filter() {
    let (_directory, repo) = fixture();
    let (base, feature, _other) = diverged(&repo);
    let page = repo.snapshot(Some(1)).unwrap();
    let mut filter = HistoryFilter::default();
    filter.hide("refs/heads/feature");
    let grown = repo.snapshot_page(&page, Some(100), &filter).unwrap();
    let ids: Vec<_> = grown
        .commits
        .iter()
        .map(|commit| commit.id.as_str())
        .collect();
    assert!(!ids.contains(&feature.as_str()));
    assert!(ids.contains(&base.as_str()));
}

#[test]
fn custom_internal_refs_are_excluded_from_history() {
    let (_directory, repo) = fixture();
    let main_commit = commit_file(&repo, "main.txt", "main\n", "Main");
    git(&repo, &["checkout", "-b", "tmp"]);
    let multica = commit_file(&repo, "multica.txt", "multica\n", "Multica record");
    git(
        &repo,
        &["update-ref", "refs/multica/local-state/agent/test", &multica],
    );
    let noted = commit_file(&repo, "noted.txt", "noted\n", "Noted record");
    git(&repo, &["update-ref", "refs/notes/commits", &noted]);
    git(&repo, &["checkout", "main"]);
    git(&repo, &["branch", "-D", "tmp"]);

    let default_ids: Vec<_> = repo
        .snapshot(None)
        .unwrap()
        .commits
        .into_iter()
        .map(|commit| commit.id)
        .collect();
    assert!(default_ids.contains(&main_commit));
    assert!(!default_ids.contains(&multica));
    assert!(!default_ids.contains(&noted));

    let filtered_ids = filtered(&repo, &[], &[]);
    assert!(filtered_ids.contains(&main_commit));
    assert!(!filtered_ids.contains(&multica));
    assert!(!filtered_ids.contains(&noted));
}

#[test]
fn hiding_remote_and_nested_branches_removes_their_exclusive_commits() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base.txt", "base\n", "Base");
    git(&repo, &["checkout", "-b", "topic/sub", &base]);
    let nested = commit_file(&repo, "nested.txt", "nested\n", "Nested");
    git(&repo, &["checkout", "--detach", &base]);
    let remote_only = commit_file(&repo, "remote.txt", "remote\n", "Remote only");
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/remote-only", &remote_only],
    );
    let tagged_only = commit_file(&repo, "tagged.txt", "tagged\n", "Tagged only");
    git(&repo, &["tag", "v-only", &tagged_only]);
    git(&repo, &["checkout", "main"]);

    let visible = filtered(&repo, &[], &[]);
    for id in [&base, &nested, &remote_only, &tagged_only, &head(&repo)] {
        assert!(visible.contains(id), "expected {id} in {visible:?}");
    }

    let hidden = filtered(
        &repo,
        &[
            "refs/heads/topic/sub",
            "refs/remotes/origin/remote-only",
            "refs/tags/v-only",
        ],
        &[],
    );
    assert!(!hidden.contains(&nested));
    assert!(!hidden.contains(&remote_only));
    assert!(!hidden.contains(&tagged_only));
    assert!(hidden.contains(&base));
    assert!(hidden.contains(&head(&repo)));
}
