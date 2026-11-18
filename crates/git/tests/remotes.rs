mod support;

use std::fs;

use ritkraken_git::{Error, NewRemote, RuleError};
use support::{command, fixture, git_output};

#[test]
fn remote_input_rejects_missing_values_and_nul_before_any_write() {
    assert!(matches!(
        NewRemote::new(" ", "url", None),
        Err(Error::Rule(RuleError::InvalidRemoteName))
    ));
    assert!(matches!(
        NewRemote::new("origin", " ", None),
        Err(Error::Rule(RuleError::EmptyRemoteUrl))
    ));
    assert!(matches!(
        NewRemote::new("origin", "url", Some("bad\0url")),
        Err(Error::Rule(RuleError::InvalidRemoteUrl))
    ));
    assert!(matches!(
        NewRemote::new("origin", "bad\0url", None),
        Err(Error::Rule(RuleError::InvalidRemoteUrl))
    ));
}

#[test]
fn adding_a_remote_preserves_distinct_pull_and_push_urls() {
    let (_directory, repo) = fixture();
    let remote = NewRemote::new(
        " upstream ",
        " https://example.invalid/team/read.git ",
        Some(" git@example.invalid:team/write.git "),
    )
    .unwrap();
    repo.add_remote(&remote).unwrap();
    for (key, expected) in [
        (
            "remote.upstream.url",
            "https://example.invalid/team/read.git",
        ),
        (
            "remote.upstream.pushurl",
            "git@example.invalid:team/write.git",
        ),
        (
            "remote.upstream.fetch",
            "+refs/heads/*:refs/remotes/upstream/*",
        ),
    ] {
        let output =
            git_output(command(Some(repo.root())).args(["config", "--local", "--get", key]));
        assert_eq!(String::from_utf8(output).unwrap().trim(), expected);
    }
    assert_eq!(repo.snapshot(None).unwrap().remotes, ["upstream"]);
}

#[test]
fn blank_push_url_uses_the_pull_url_and_local_paths_remain_valid() {
    let (_directory, repo) = fixture();
    repo.add_remote(&NewRemote::new("team/mirror", "../a repository", Some(" ")).unwrap())
        .unwrap();
    let output =
        git_output(command(Some(repo.root())).args(["remote", "get-url", "--push", "team/mirror"]));
    assert_eq!(String::from_utf8(output).unwrap().trim(), "../a repository");
    let output = command(Some(repo.root()))
        .args(["config", "--local", "--get", "remote.team/mirror.pushurl"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn duplicate_remote_names_never_replace_existing_configuration() {
    let (_directory, repo) = fixture();
    repo.add_remote(&NewRemote::new("origin", "original-pull", Some("original-push")).unwrap())
        .unwrap();
    assert!(
        repo.add_remote(
            &NewRemote::new("origin", "replacement-pull", Some("replacement-push")).unwrap()
        )
        .is_err()
    );
    for (key, expected) in [
        ("remote.origin.url", "original-pull"),
        ("remote.origin.pushurl", "original-push"),
    ] {
        let output =
            git_output(command(Some(repo.root())).args(["config", "--local", "--get", key]));
        assert_eq!(String::from_utf8(output).unwrap().trim(), expected);
    }
}

#[test]
fn updating_a_remote_rewrites_urls_and_renames() {
    let (_directory, repo) = fixture();
    repo.add_remote(&NewRemote::new("origin", "pull-a", Some("push-a")).unwrap())
        .unwrap();
    assert_eq!(
        repo.remote_urls("origin").unwrap(),
        ("pull-a".to_owned(), Some("push-a".to_owned()))
    );
    repo.update_remote(
        "origin",
        &NewRemote::new("upstream", "pull-b", None).unwrap(),
    )
    .unwrap();
    assert_eq!(repo.remote_urls("upstream").unwrap(), ("pull-b".to_owned(), None));
    assert_eq!(repo.snapshot(None).unwrap().remotes, ["upstream"]);
    for (key, expected) in [
        ("remote.upstream.url", "pull-b"),
        ("remote.upstream.fetch", "+refs/heads/*:refs/remotes/upstream/*"),
    ] {
        let output =
            git_output(command(Some(repo.root())).args(["config", "--local", "--get", key]));
        assert_eq!(String::from_utf8(output).unwrap().trim(), expected);
    }
    // The cleared push URL leaves no configured value behind.
    let output = command(Some(repo.root()))
        .args(["config", "--local", "--get", "remote.upstream.pushurl"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn updating_a_remote_rejects_a_name_collision_before_writing() {
    let (_directory, repo) = fixture();
    repo.add_remote(&NewRemote::new("origin", "pull-a", Some("push-a")).unwrap())
        .unwrap();
    repo.add_remote(&NewRemote::new("backup", "pull-c", None).unwrap())
        .unwrap();
    assert!(matches!(
        repo.update_remote("origin", &NewRemote::new("backup", "pull-d", None).unwrap()),
        Err(Error::Rule(RuleError::RemoteExists))
    ));
    assert_eq!(
        repo.remote_urls("origin").unwrap(),
        ("pull-a".to_owned(), Some("push-a".to_owned()))
    );
}

#[test]
fn removing_a_remote_requires_it_to_exist() {
    let (_directory, repo) = fixture();
    repo.add_remote(&NewRemote::new("origin", "pull-a", None).unwrap())
        .unwrap();
    repo.remove_remote("origin").unwrap();
    assert!(repo.snapshot(None).unwrap().remotes.is_empty());
    assert!(matches!(
        repo.remove_remote("origin"),
        Err(Error::Rule(RuleError::MissingRemote(name))) if name == "origin"
    ));
}

#[test]
fn invalid_names_and_locked_configuration_do_not_leave_a_remote() {
    let (_directory, repo) = fixture();
    assert!(
        repo.add_remote(&NewRemote::new("bad name", "url", None).unwrap())
            .is_err()
    );
    assert!(repo.snapshot(None).unwrap().remotes.is_empty());
    let lock = repo.root().join(".git").join("config.lock");
    fs::write(&lock, "held by test").unwrap();
    assert!(
        repo.add_remote(&NewRemote::new("origin", "url", Some("push")).unwrap())
            .is_err()
    );
    fs::remove_file(lock).unwrap();
    assert!(repo.snapshot(None).unwrap().remotes.is_empty());
}
