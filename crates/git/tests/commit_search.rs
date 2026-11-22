mod support;

use ritkraken_git::{CommitSearch, Error, Repo, RuleError};
use support::{
    command,
    commits::{git, head},
    fixture, git_output,
};

fn commit(repo: &Repo, path: &str, message: &str, date: &str) -> String {
    std::fs::write(repo.root().join(path), message).unwrap();
    git(repo, &["add", "--", path]);
    git_output(
        command(Some(repo.root()))
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .args(["commit", "-m", message]),
    );
    head(repo)
}

#[test]
fn literal_filters_search_full_messages_and_combine_with_and() {
    let (_directory, repo) = fixture();
    let old = commit(&repo, "old.txt", "Ancient history", "2020-01-01 12:00:00");
    git(&repo, &["config", "user.name", "Alice Example"]);
    git(
        &repo,
        &["config", "user.email", "alice+build@example.invalid"],
    );
    let body = commit(
        &repo,
        "report[1].txt",
        "Report\n\nHidden NEEDLE.* body",
        "2024-02-29 23:59:59",
    );
    git(&repo, &["branch", "topic"]);
    git(&repo, &["update-ref", "refs/remotes/origin/topic", &body]);
    git(&repo, &["config", "user.name", "Bob Other"]);
    git(&repo, &["config", "user.email", "bob@example.invalid"]);
    let recent = commit(&repo, "--all", "Newest report", "2024-03-01 00:00:00");
    assert_eq!(repo.snapshot(Some(1)).unwrap().commits.len(), 1);
    let sha = body[..9].to_uppercase();
    let cases = [
        (["aNcIeNt", "", "", "", "", ""], vec![old.clone()]),
        (["needle.*", "", "", "", "", ""], vec![body.clone()]),
        ([&sha, "", "", "", "", ""], vec![body.clone()]),
        (["needle.+", "", "", "", "", ""], vec![]),
        (["", "ALICE", "", "", "", ""], vec![body.clone()]),
        (["", "+BUILD@EXAMPLE", "", "", "", ""], vec![body.clone()]),
        (["", "Alice.*", "", "", "", ""], vec![]),
        (["", "", "report[1].txt", "", "", ""], vec![body.clone()]),
        (["", "", "--all", "", "", ""], vec![recent.clone()]),
        (["", "", ":(glob)**", "", "", ""], vec![]),
        (
            ["", "", "", "topic", "", ""],
            vec![body.clone(), old.clone()],
        ),
        (
            ["", "", "", "origin/topic", "", ""],
            vec![body.clone(), old.clone()],
        ),
        (
            ["", "", "", "refs/remotes/origin/topic", "", ""],
            vec![body.clone(), old.clone()],
        ),
        (["", "", "", "", "2024-03-01", ""], vec![recent]),
        (["", "", "", "", "", "2024-02-29"], vec![body.clone(), old]),
        (
            [
                "needle.*",
                "alice",
                "report[1].txt",
                "refs/heads/topic",
                "2024-02-29",
                "2024-02-29",
            ],
            vec![body],
        ),
        (["needle", "bob", "", "", "", ""], vec![]),
    ];
    for ([text, author, path, branch, since, until], expected) in cases {
        let search = CommitSearch {
            text: text.into(),
            author: author.into(),
            path: path.into(),
            branch: branch.into(),
            since: since.into(),
            until: until.into(),
        };
        let found = repo.search_commits(&search).unwrap();
        assert_eq!(
            found.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            expected,
            "{search:?}"
        );
    }
}

#[test]
fn default_search_includes_detached_head_and_other_refs_but_excludes_stash() {
    let (_directory, repo) = fixture();
    let base = commit(&repo, "base", "Base", "2024-01-01 12:00:00");
    git(&repo, &["checkout", "--detach"]);
    let detached = commit(&repo, "detached", "Detached", "2024-01-02 12:00:00");
    std::fs::write(repo.root().join("base"), "stash change").unwrap();
    git(&repo, &["stash", "push", "-m", "Stash only"]);
    let found = repo.search_commits(&CommitSearch::default()).unwrap();
    assert_eq!(
        found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        vec![detached.as_str(), base.as_str()]
    );
}

#[test]
fn search_includes_commits_reachable_only_from_tags_and_remote_branches() {
    let (_directory, repo) = fixture();
    let base = commit(&repo, "base", "Base", "2024-01-01 12:00:00");
    let tagged = commit(&repo, "tagged", "Tagged", "2024-01-02 12:00:00");
    git(&repo, &["tag", "v1"]);
    git(&repo, &["checkout", "--detach", &base]);
    let remote = commit(&repo, "remote", "Remote", "2024-01-03 12:00:00");
    git(&repo, &["update-ref", "refs/remotes/origin/only", &remote]);
    git(&repo, &["checkout", "-B", "main", &base]);
    let found = repo.search_commits(&CommitSearch::default()).unwrap();
    let mut ids = found.into_iter().map(|c| c.id).collect::<Vec<_>>();
    ids.sort();
    let mut expected = vec![base, tagged, remote];
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn empty_and_unborn_repositories_return_no_commits() {
    let (_directory, repo) = fixture();
    for search in [
        CommitSearch::default(),
        CommitSearch {
            text: "anything".into(),
            path: "missing".into(),
            ..Default::default()
        },
    ] {
        assert!(repo.search_commits(&search).unwrap().is_empty());
    }
}

#[test]
fn malformed_dates_branches_and_paths_are_rejected_before_git_log() {
    let (_directory, repo) = fixture();
    for date in [
        "2024-2-01",
        "2023-02-29",
        "2024-04-31",
        "2024-00-01",
        "2024-01-00",
        "0000-01-01",
        "--all",
        "2024-01-01\n",
        "999x-01-01",
    ] {
        for search in [
            CommitSearch {
                since: date.into(),
                ..Default::default()
            },
            CommitSearch {
                until: date.into(),
                ..Default::default()
            },
        ] {
            assert!(
                matches!(
                    repo.search_commits(&search),
                    Err(Error::Rule(RuleError::InvalidSearchDate))
                ),
                "{search:?}"
            );
        }
    }
    for branch in [
        "--all",
        "-n1",
        "main..other",
        "HEAD",
        "main\n",
        "refs/tags/v1",
    ] {
        let search = CommitSearch {
            branch: branch.into(),
            ..Default::default()
        };
        assert!(
            matches!(
                repo.search_commits(&search),
                Err(Error::Rule(
                    RuleError::InvalidRefName | RuleError::RefUnavailable
                ))
            ),
            "{search:?}"
        );
    }
    let missing = CommitSearch {
        branch: "unknown".into(),
        ..Default::default()
    };
    assert!(matches!(
        repo.search_commits(&missing),
        Err(Error::Rule(RuleError::RefUnavailable))
    ));
    for path in ["../outside", "/outside", "C:\\outside", "a\0b"] {
        let search = CommitSearch {
            path: path.into(),
            ..Default::default()
        };
        assert!(
            matches!(
                repo.search_commits(&search),
                Err(Error::Rule(RuleError::InvalidFilePath))
            ),
            "{search:?}"
        );
    }
}

#[test]
fn default_search_excludes_custom_internal_refs() {
    let (_directory, repo) = fixture();
    let base = commit(&repo, "base", "Base", "2024-01-01 12:00:00");
    git(&repo, &["checkout", "--detach"]);
    let detached = commit(&repo, "detached", "Detached", "2024-01-02 12:00:00");
    git(&repo, &["checkout", "-b", "tmp", &base]);
    let custom = commit(&repo, "custom", "Multica record", "2024-01-03 12:00:00");
    git(
        &repo,
        &["update-ref", "refs/multica/local-state/agent/test", &custom],
    );
    git(&repo, &["checkout", "--detach", &detached]);
    git(&repo, &["branch", "-D", "tmp"]);
    let remote = commit(&repo, "remote", "Remote", "2024-01-04 12:00:00");
    git(&repo, &["update-ref", "refs/remotes/origin/topic", &remote]);
    git(&repo, &["checkout", "--detach", &base]);
    let tagged = commit(&repo, "tagged", "Tagged", "2024-01-05 12:00:00");
    git(&repo, &["tag", "v1", &tagged]);
    git(&repo, &["checkout", "--detach", &detached]);

    let found = repo.search_commits(&CommitSearch::default()).unwrap();
    let mut ids: Vec<_> = found.into_iter().map(|commit| commit.id).collect();
    ids.sort();
    let mut expected = vec![base, detached, remote, tagged];
    expected.sort();
    assert!(!ids.contains(&custom));
    assert_eq!(ids, expected);
}
