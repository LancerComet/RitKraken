mod support;

use std::fs;

use ritkraken_git::{Error, PullMode, Repo, RuleError, Upstream};
use support::commits::{commit_file, git, head};
use support::{command, fixture, git_output};

fn upstream() -> Upstream {
    Upstream {
        remote: "origin".into(),
        reference: "refs/heads/main".into(),
    }
}

/// Publishes `main`, then rewinds the local branch so it is one commit behind.
/// Returns the published commit that a pull should bring back.
fn behind() -> (tempfile::TempDir, Repo, String) {
    let (directory, repo) = fixture();
    let remote = directory.path().join("remote.git");
    git_output(command(None).args(["init", "--bare"]).arg(&remote));
    git_output(
        command(Some(repo.root()))
            .args(["remote", "add", "origin"])
            .arg(&remote),
    );
    commit_file(&repo, "base", "base", "Base");
    repo.push("main", &upstream(), true).unwrap();
    let remote_tip = commit_file(&repo, "remote", "remote", "Remote");
    repo.push("main", &upstream(), false).unwrap();
    git(&repo, &["reset", "--hard", "HEAD~1"]);
    (directory, repo, remote_tip)
}

fn diverged() -> (tempfile::TempDir, Repo, String, String) {
    let (directory, repo, remote_tip) = behind();
    let local = commit_file(&repo, "local", "local", "Local");
    (directory, repo, remote_tip, local)
}

fn parents(repo: &Repo) -> Vec<String> {
    git(repo, &["rev-list", "--parents", "-n", "1", "HEAD"])
        .split_whitespace()
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_mode_fast_forwards_a_branch_that_is_only_behind() {
    for mode in [
        PullMode::FastForwardOrMerge,
        PullMode::FastForwardOnly,
        PullMode::Rebase,
    ] {
        let (_directory, repo, remote_tip) = behind();
        repo.pull("main", &upstream(), mode).unwrap();
        assert_eq!(head(&repo), remote_tip, "{mode:?}");
    }
}

#[test]
fn fast_forward_or_merge_merges_diverged_history_despite_conflicting_config() {
    let (_directory, repo, remote_tip, local) = diverged();
    git(&repo, &["config", "pull.rebase", "true"]);
    git(&repo, &["config", "pull.ff", "only"]);
    repo.pull("main", &upstream(), PullMode::FastForwardOrMerge)
        .unwrap();
    assert_eq!(parents(&repo), [local, remote_tip]);
}

#[test]
fn fast_forward_only_leaves_diverged_history_untouched() {
    let (_directory, repo, _remote_tip, local) = diverged();
    git(&repo, &["config", "pull.rebase", "true"]);
    let error = repo
        .pull("main", &upstream(), PullMode::FastForwardOnly)
        .unwrap_err();
    assert!(matches!(error, Error::Git { .. }), "{error:?}");
    assert_eq!(head(&repo), local);
}

#[test]
fn rebase_replays_local_commits_on_the_upstream_tip() {
    let (_directory, repo, remote_tip, local) = diverged();
    git(&repo, &["config", "pull.rebase", "false"]);
    repo.pull("main", &upstream(), PullMode::Rebase).unwrap();
    assert_ne!(head(&repo), local);
    assert_eq!(parents(&repo), [remote_tip]);
    assert_eq!(git(&repo, &["log", "-1", "--format=%s"]).trim(), "Local");
}

#[test]
fn pull_never_stashes_conflicting_working_files() {
    let (_directory, repo, _remote_tip) = behind();
    // The upstream also changes the tracked file the user is editing, so only
    // an implicit stash could let the pull proceed.
    git(&repo, &["reset", "-q", "--hard", "origin/main"]);
    commit_file(&repo, "base", "upstream base", "Upstream base");
    repo.push("main", &upstream(), false).unwrap();
    git(&repo, &["reset", "--hard", "HEAD~2"]);
    let before = head(&repo);
    git(&repo, &["config", "rebase.autoStash", "true"]);
    git(&repo, &["config", "merge.autoStash", "true"]);
    fs::write(repo.root().join("base"), "unsaved").unwrap();
    for mode in [PullMode::FastForwardOrMerge, PullMode::Rebase] {
        assert!(repo.pull("main", &upstream(), mode).is_err(), "{mode:?}");
        assert_eq!(head(&repo), before, "{mode:?}");
        assert_eq!(
            fs::read_to_string(repo.root().join("base")).unwrap(),
            "unsaved"
        );
        assert_eq!(git(&repo, &["stash", "list"]), "");
    }
}

#[test]
fn pull_refuses_a_branch_or_upstream_that_changed_after_review() {
    let (_directory, repo, _remote_tip) = behind();
    let before = head(&repo);
    let other = Upstream {
        remote: "origin".into(),
        reference: "refs/heads/other".into(),
    };
    assert!(matches!(
        repo.pull("main", &other, PullMode::FastForwardOrMerge),
        Err(Error::Rule(RuleError::UpstreamChanged))
    ));
    assert!(matches!(
        repo.pull("feature", &upstream(), PullMode::FastForwardOrMerge),
        Err(Error::Rule(RuleError::BranchChanged))
    ));
    assert_eq!(head(&repo), before);
}
