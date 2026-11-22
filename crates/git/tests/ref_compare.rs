mod support;

use ritkraken_git::Repo;
use support::commits::{commit_file, head};
use support::{command, fixture, git_output};

fn git(repo: &Repo, args: &[&str]) {
    git_output(command(Some(repo.root())).args(args));
}

/// main <- fork: shared root, then `feature` diverges by one commit and
/// `main` advances by one commit.
fn diverged(repo: &Repo) -> (String, String, String) {
    let fork = commit_file(repo, "shared.txt", "shared\n", "Base");
    git(repo, &["checkout", "-b", "feature"]);
    let feature = commit_file(repo, "feature.txt", "feature\n", "Feature work");
    git(repo, &["checkout", "main"]);
    let main = commit_file(repo, "main.txt", "main\n", "Main work");
    (fork, feature, main)
}

#[test]
fn diverged_branches_list_each_side_and_diff_against_the_merge_base() {
    let (_directory, repo) = fixture();
    let (fork, feature, main) = diverged(&repo);
    let comparison = repo
        .compare_refs("refs/heads/main", "refs/heads/feature", 100)
        .unwrap();
    assert_eq!(comparison.merge_base.as_deref(), Some(fork.as_str()));
    assert_eq!(
        comparison
            .only_base
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        [main.as_str()]
    );
    assert_eq!(
        comparison
            .only_compare
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        [feature.as_str()]
    );
    assert!(!comparison.base_limited && !comparison.compare_limited);
    let patch = String::from_utf8(comparison.patch.unwrap()).unwrap();
    assert!(patch.contains("feature.txt"), "{patch}");
    assert!(!patch.contains("main.txt"), "{patch}");
}

#[test]
fn identical_refs_have_empty_lists_and_patch() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a.txt", "a\n", "A");
    let comparison = repo.compare_refs("refs/heads/main", "HEAD", 100).unwrap();
    assert_eq!(comparison.merge_base, repo.snapshot(None).unwrap().head);
    assert!(comparison.only_base.is_empty() && comparison.only_compare.is_empty());
    assert_eq!(comparison.patch, Some(Vec::new()));
}

#[test]
fn an_ancestor_compare_side_brings_nothing_new() {
    let (_directory, repo) = fixture();
    let first = commit_file(&repo, "a.txt", "a\n", "A");
    git(&repo, &["branch", "old", &first]);
    commit_file(&repo, "b.txt", "b\n", "B");
    let comparison = repo
        .compare_refs("refs/heads/main", "refs/heads/old", 100)
        .unwrap();
    assert_eq!(comparison.merge_base.as_deref(), Some(first.as_str()));
    assert_eq!(comparison.only_base.len(), 1);
    assert!(comparison.only_compare.is_empty());
    // The merge base equals the compare side, so the patch is empty.
    assert_eq!(comparison.patch, Some(Vec::new()));
}

#[test]
fn annotated_tags_peel_to_their_commit() {
    let (_directory, repo) = fixture();
    let first = commit_file(&repo, "a.txt", "a\n", "A");
    git(&repo, &["tag", "-a", "v1", "-m", "release", &first]);
    commit_file(&repo, "b.txt", "b\n", "B");
    let comparison = repo
        .compare_refs("refs/heads/main", "refs/tags/v1", 100)
        .unwrap();
    assert_eq!(comparison.merge_base.as_deref(), Some(first.as_str()));
    assert_eq!(comparison.only_base.len(), 1);
    assert!(comparison.only_compare.is_empty());
}

#[test]
fn unrelated_histories_fall_back_to_a_direct_diff() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "main.txt", "main\n", "Main");
    git(&repo, &["checkout", "--orphan", "other"]);
    git(&repo, &["rm", "-rf", "-q", "."]);
    commit_file(&repo, "other.txt", "other\n", "Other");
    let comparison = repo
        .compare_refs("refs/heads/main", "refs/heads/other", 100)
        .unwrap();
    assert_eq!(comparison.merge_base, None);
    assert_eq!(comparison.only_base.len(), 1);
    assert_eq!(comparison.only_compare.len(), 1);
    let patch = String::from_utf8(comparison.patch.unwrap()).unwrap();
    assert!(patch.contains("other.txt"), "{patch}");
}

#[test]
fn the_limit_flags_when_a_side_exceeds_it() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a.txt", "a\n", "A");
    git(&repo, &["checkout", "-b", "feature"]);
    for index in 0..3 {
        commit_file(
            &repo,
            "a.txt",
            &format!("{index}\n"),
            &format!("Feature {index}"),
        );
    }
    let comparison = repo
        .compare_refs("refs/heads/main", "refs/heads/feature", 1)
        .unwrap();
    assert_eq!(comparison.only_compare.len(), 1);
    assert!(comparison.compare_limited);
    assert!(!comparison.base_limited);
}

#[test]
fn invalid_revisions_are_rejected() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a.txt", "a\n", "A");
    assert!(repo.compare_refs("refs/heads/nosuch", "HEAD", 10).is_err());
    assert!(repo.compare_refs("HEAD", "refs/tags/missing", 10).is_err());
    assert!(repo.compare_refs("HEAD", "HEAD", 10).is_ok());
}

#[test]
fn commit_diff_reads_normal_root_and_merge_commits() {
    let (_directory, repo) = fixture();
    let root = commit_file(&repo, "root.txt", "root\n", "Root");
    commit_file(&repo, "normal.txt", "normal\n", "Normal");
    // Root commit: the whole tree as an addition.
    let patch = String::from_utf8(repo.commit_diff(&root).unwrap().unwrap()).unwrap();
    assert!(patch.contains("root.txt"), "{patch}");
    // A normal commit diffs against its parent.
    let normal = head(&repo);
    let patch = String::from_utf8(repo.commit_diff(&normal).unwrap().unwrap()).unwrap();
    assert!(patch.contains("normal.txt"), "{patch}");
    // A merge commit diffs against its first parent.
    git(&repo, &["checkout", "-b", "feature"]);
    commit_file(&repo, "merged.txt", "merged\n", "Merged");
    git(&repo, &["checkout", "main"]);
    git(
        &repo,
        &["merge", "--no-ff", "-m", "Merge feature", "feature"],
    );
    let merge = head(&repo);
    let patch = String::from_utf8(repo.commit_diff(&merge).unwrap().unwrap()).unwrap();
    assert!(patch.contains("merged.txt"), "{patch}");
    assert!(repo.commit_diff("not-a-sha").is_err());
}
