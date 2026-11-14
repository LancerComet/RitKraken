mod support;

use ritkraken_git::{NewRemote, RefKind};
use support::commits::{commit_file, git, head};
use support::{command, fixture, git_output};

#[derive(Clone, Copy, Debug)]
enum FetchMode {
    Selected,
    All,
    Automatic,
}

#[test]
fn selected_fetch_prunes_only_the_selected_remote() {
    assert_pruning(FetchMode::Selected);
}

#[test]
fn fetch_all_prunes_deleted_branches_on_every_remote() {
    assert_pruning(FetchMode::All);
}

#[test]
fn automatic_fetch_also_prunes_deleted_remote_branches() {
    assert_pruning(FetchMode::Automatic);
}

fn assert_pruning(mode: FetchMode) {
    let (_checkout, repo) = fixture();
    let servers = tempfile::tempdir().unwrap();
    let tip = commit_file(&repo, "base.txt", "base\n", "Base");
    git(&repo, &["config", "--local", "fetch.prune", "false"]);
    git(&repo, &["config", "--local", "fetch.pruneTags", "false"]);
    for name in ["origin", "upstream"] {
        let server = servers.path().join(format!("{name}.git"));
        git_output(
            command(None)
                .args(["init", "--bare", "--quiet"])
                .arg(&server),
        );
        git_output(command(Some(&server)).args(["config", "core.hooksPath", "disabled-hooks"]));
        repo.add_remote(&NewRemote::new(name, &server.to_string_lossy(), None).unwrap())
            .unwrap();
        git(
            &repo,
            &[
                "config",
                "--local",
                &format!("remote.{name}.prune"),
                "false",
            ],
        );
        git(
            &repo,
            &[
                "config",
                "--local",
                &format!("remote.{name}.pruneTags"),
                "false",
            ],
        );
        git(
            &repo,
            &[
                "push",
                name,
                "main:refs/heads/main",
                "main:refs/heads/obsolete",
            ],
        );
    }
    repo.fetch(None).unwrap();
    git(&repo, &["branch", "--track", "retained", "origin/obsolete"]);
    git(&repo, &["tag", "local-only"]);
    let before = repo.snapshot(None).unwrap();
    assert_eq!(
        remote_names(&before),
        [
            "origin/main",
            "origin/obsolete",
            "upstream/main",
            "upstream/obsolete"
        ]
    );

    // Simulate a collaborator deleting the branches directly on both servers.
    for name in ["origin", "upstream"] {
        git_output(
            command(Some(&servers.path().join(format!("{name}.git")))).args([
                "update-ref",
                "-d",
                "refs/heads/obsolete",
            ]),
        );
    }
    match mode {
        FetchMode::Selected => repo.fetch(Some("origin")),
        FetchMode::All => repo.fetch(None),
        FetchMode::Automatic => repo.auto_fetch(),
    }
    .unwrap();

    let after = repo.snapshot(None).unwrap();
    let expected = match mode {
        FetchMode::Selected => vec!["origin/main", "upstream/main", "upstream/obsolete"],
        FetchMode::All | FetchMode::Automatic => vec!["origin/main", "upstream/main"],
    };
    assert_eq!(remote_names(&after), expected, "{mode:?}");
    assert_eq!(after.branch, before.branch);
    assert_eq!(after.status, before.status);
    assert_eq!(head(&repo), tip);
    for reference in ["refs/heads/retained", "refs/tags/local-only"] {
        assert_eq!(git(&repo, &["rev-parse", reference]).trim(), tip);
    }
    assert_eq!(
        git(&repo, &["config", "branch.retained.remote"]).trim(),
        "origin"
    );
}

fn remote_names(snapshot: &ritkraken_git::Snapshot) -> Vec<&str> {
    snapshot
        .refs_of_kind(RefKind::Remote)
        .map(|reference| reference.name.as_str())
        .collect()
}
