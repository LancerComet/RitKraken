mod support;

use ritkraken_git::Repo;
use support::commits::git;
use support::{command, fixture, git_output};

/// Commits `name` with the same author and committer time, `hours` after 2026-10-10 UTC.
fn commit_at(repo: &Repo, name: &str, hours: u32) -> String {
    std::fs::write(repo.root().join(name), name).unwrap();
    git(repo, &["add", "--", name]);
    let stamp = 1_791_590_400 + u64::from(hours) * 3600;
    git_output(
        command(Some(repo.root()))
            .env("GIT_AUTHOR_DATE", format!("@{stamp} +0000"))
            .env("GIT_COMMITTER_DATE", format!("@{stamp} +0000"))
            .args(["commit", "-q", "-m", name]),
    );
    git(repo, &["rev-parse", "HEAD"]).trim().to_owned()
}

#[test]
fn history_lists_commits_from_different_branches_by_commit_time() {
    let (_directory, repo) = fixture();
    let base = commit_at(&repo, "base", 0);
    git(&repo, &["switch", "-q", "-c", "side"]);
    let side_old = commit_at(&repo, "side-old", 2);
    let side_new = commit_at(&repo, "side-new", 4);
    git(&repo, &["switch", "-q", "main"]);
    let main_old = commit_at(&repo, "main-old", 1);
    let main_new = commit_at(&repo, "main-new", 3);

    let order: Vec<_> = repo
        .snapshot(None)
        .unwrap()
        .commits
        .into_iter()
        .map(|commit| commit.id)
        .collect();
    assert_eq!(order, [side_new, main_new, side_old, main_old, base]);
}
