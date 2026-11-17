mod support;

use std::{fs, path::PathBuf};

use ritkraken_git::{Error, PushRejection, Repo, RuleError, Upstream};
use support::commits::{commit_file, git, head};
use support::{command, fixture, git_output};

fn destination() -> Upstream {
    Upstream {
        remote: "origin".into(),
        reference: "refs/heads/published".into(),
    }
}

fn published() -> (tempfile::TempDir, Repo, PathBuf) {
    let (directory, repo) = fixture();
    let remote = directory.path().join("remote.git");
    git_output(command(None).args(["init", "--bare"]).arg(&remote));
    git_output(
        command(Some(repo.root()))
            .args(["remote", "add", "origin"])
            .arg(&remote),
    );
    commit_file(&repo, "base", "base", "Base");
    commit_file(&repo, "published", "published", "Published");
    repo.push("main", &destination(), true).unwrap();
    (directory, repo, remote)
}

fn diverged() -> (tempfile::TempDir, Repo, PathBuf) {
    let fixture = published();
    git(&fixture.1, &["commit", "--amend", "-m", "Rewritten"]);
    fixture
}

fn rejected(repo: &Repo, set_upstream: bool) -> PushRejection {
    match repo.push("main", &destination(), set_upstream) {
        Err(Error::PushRejected(request)) => *request,
        other => panic!("Expected a non-fast-forward rejection, got {other:?}"),
    }
}

fn remote_head(remote: &std::path::Path) -> String {
    String::from_utf8(git_output(
        command(Some(remote)).args(["rev-parse", "refs/heads/published"]),
    ))
    .unwrap()
    .trim()
    .to_owned()
}

#[test]
fn ordinary_push_stays_fast_forward_and_only_rejection_requests_confirmation() {
    let (_directory, repo, remote) = published();
    commit_file(&repo, "next", "next", "Next");
    repo.push("main", &destination(), false).unwrap();
    let published = remote_head(&remote);
    git(&repo, &["commit", "--amend", "-m", "Rewritten"]);
    let local = head(&repo);
    let request = rejected(&repo, false);
    assert!(request.matches_snapshot(&repo.snapshot(None).unwrap()));
    assert_eq!(remote_head(&remote), published);
    assert_eq!(head(&repo), local);
}

#[test]
fn confirmation_force_pushes_only_the_selected_branch_without_following_tags() {
    let (_directory, repo, remote) = diverged();
    let published = remote_head(&remote);
    git_output(command(Some(&remote)).args(["update-ref", "refs/heads/other", &published]));
    git(&repo, &["config", "push.followTags", "true"]);
    git(
        &repo,
        &[
            "config",
            "remote.origin.push",
            "refs/heads/main:refs/heads/other",
        ],
    );
    git(&repo, &["tag", "-a", "new-tag", "-m", "New tag"]);
    let request = rejected(&repo, false);
    repo.force_push(&request).unwrap();
    assert_eq!(remote_head(&remote), head(&repo));
    let other = git_output(command(Some(&remote)).args(["rev-parse", "refs/heads/other"]));
    assert_eq!(String::from_utf8(other).unwrap().trim(), published);
    assert!(
        !command(Some(&remote))
            .args(["show-ref", "--verify", "refs/tags/new-tag"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn a_remote_advance_is_not_overwritten_even_after_background_fetch() {
    let (_directory, repo, remote) = diverged();
    let rewritten = head(&repo);
    let future = commit_file(&repo, "future", "future", "Future");
    git(&repo, &["push", "origin", "main:refs/heads/other"]);
    git(&repo, &["reset", "--hard", &rewritten]);
    let request = rejected(&repo, false);
    git_output(command(Some(&remote)).args(["update-ref", "refs/heads/published", &future]));
    repo.fetch(Some("origin")).unwrap();
    assert!(repo.force_push(&request).is_err());
    assert_eq!(remote_head(&remote), future);
    assert_eq!(head(&repo), rewritten);
}

#[test]
fn confirmation_rechecks_the_local_head_and_checkout() {
    let (_directory, repo, remote) = diverged();
    let request = rejected(&repo, false);
    let published = remote_head(&remote);
    git(&repo, &["commit", "--amend", "-m", "Changed again"]);
    assert!(matches!(
        repo.force_push(&request),
        Err(Error::Rule(RuleError::BranchChanged))
    ));
    let request = rejected(&repo, false);
    git(&repo, &["switch", "-c", "other"]);
    assert!(matches!(
        repo.force_push(&request),
        Err(Error::Rule(RuleError::BranchChanged))
    ));
    assert_eq!(remote_head(&remote), published);
}

#[test]
fn confirmation_rechecks_upstream_and_the_configured_push_url() {
    let (_directory, repo, remote) = diverged();
    let request = rejected(&repo, false);
    git(&repo, &["config", "branch.main.merge", "refs/heads/other"]);
    assert!(matches!(
        repo.force_push(&request),
        Err(Error::Rule(RuleError::UpstreamChanged))
    ));
    git(
        &repo,
        &["config", "branch.main.merge", "refs/heads/published"],
    );
    git(
        &repo,
        &["config", "remote.origin.pushurl", "missing-remote"],
    );
    assert!(matches!(
        repo.force_push(&request),
        Err(Error::Rule(RuleError::UpstreamChanged))
    ));
    assert_ne!(remote_head(&remote), head(&repo));
}

#[test]
fn a_missing_tracking_ref_still_uses_the_actual_server_tip() {
    let (_directory, repo, remote) = diverged();
    git(
        &repo,
        &["update-ref", "-d", "refs/remotes/origin/published"],
    );
    let request = rejected(&repo, false);
    repo.force_push(&request).unwrap();
    assert_eq!(remote_head(&remote), head(&repo));
}

#[test]
fn first_push_to_an_existing_remote_branch_can_be_confirmed_and_tracked() {
    let (_directory, repo, remote) = diverged();
    git(&repo, &["branch", "--unset-upstream"]);
    let request = rejected(&repo, true);
    repo.force_push(&request).unwrap();
    assert_eq!(remote_head(&remote), head(&repo));
    assert_eq!(repo.snapshot(None).unwrap().upstream, Some(destination()));
}

#[test]
fn transport_failures_do_not_offer_force_push() {
    let (_directory, repo, _remote) = diverged();
    git(&repo, &["remote", "set-url", "origin", "missing-remote"]);
    let result = repo.push("main", &destination(), false);
    assert!(result.is_err());
    assert!(!matches!(result, Err(Error::PushRejected(_))));
}

#[test]
fn server_hook_rejections_do_not_offer_force_push() {
    let (_directory, repo, remote) = published();
    commit_file(&repo, "next", "next", "Next");
    let hook = remote.join("hooks/pre-receive");
    fs::write(
        &hook,
        "#!/bin/sh\necho '[rejected] (non-fast-forward)' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = repo.push("main", &destination(), false);
    assert!(result.is_err());
    assert!(!matches!(result, Err(Error::PushRejected(_))));
}

#[test]
fn pulling_integrates_diverged_history_without_force_pushing() {
    let (_directory, repo, remote) = diverged();
    let published = remote_head(&remote);
    let request = rejected(&repo, false);
    repo.pull_after_rejected_push(&request).unwrap();
    assert_eq!(
        git(&repo, &["rev-list", "--parents", "-n", "1", "HEAD"])
            .split_whitespace()
            .count(),
        3
    );
    assert_eq!(remote_head(&remote), published);
    repo.push("main", &destination(), false).unwrap();
    assert_eq!(remote_head(&remote), head(&repo));
}

#[test]
fn multiple_push_destinations_are_not_silently_force_pushed() {
    let (_directory, repo, remote) = diverged();
    for _ in 0..2 {
        git_output(
            command(Some(repo.root()))
                .args(["config", "--add", "remote.origin.pushurl"])
                .arg(&remote),
        );
    }
    let result = repo.push("main", &destination(), false);
    assert!(result.is_err());
    assert!(!matches!(result, Err(Error::PushRejected(_))));
}

#[test]
fn force_push_uses_the_push_url_when_fetch_and_push_destinations_differ() {
    let (directory, repo, fetch_remote) = published();
    let push_remote = directory.path().join("push.git");
    git_output(
        command(None)
            .args(["clone", "--bare"])
            .arg(&fetch_remote)
            .arg(&push_remote),
    );
    git_output(
        command(Some(repo.root()))
            .args(["remote", "set-url", "--push", "origin"])
            .arg(&push_remote),
    );
    let fetch_tip = remote_head(&fetch_remote);
    git(&repo, &["commit", "--amend", "-m", "Rewritten"]);
    let request = rejected(&repo, false);
    repo.force_push(&request).unwrap();
    assert_eq!(remote_head(&push_remote), head(&repo));
    assert_eq!(remote_head(&fetch_remote), fetch_tip);
}

#[test]
fn pull_conflicts_remain_available_to_the_existing_conflict_resolver() {
    let (_directory, repo, remote) = published();
    fs::write(repo.root().join("published"), "rewritten contents").unwrap();
    git(&repo, &["add", "published"]);
    git(&repo, &["commit", "--amend", "-m", "Rewritten"]);
    let published = remote_head(&remote);
    let request = rejected(&repo, false);
    assert!(repo.pull_after_rejected_push(&request).is_err());
    let snapshot = repo.snapshot(None).unwrap();
    assert!(snapshot.sequence.is_some());
    assert!(snapshot.status.has_conflicts());
    assert_eq!(remote_head(&remote), published);
}
