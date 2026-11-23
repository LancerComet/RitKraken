mod support;

use std::{fs, path::Path};

use support::{command, fixture, git_output};

use ritkraken_git::{
    CommitPerson, FileTarget, HistoryFilter, PATCH_VIEW_LIMIT, PatchSelection, PatchSide, Repo,
    Scope, Upstream,
};

#[test]
fn partial_stage_preserves_other_edits_and_refuses_a_stale_patch() {
    let (_directory, repo) = fixture();
    let name = "测试 file.txt";
    let path = repo.root().join(name);
    let original = (1..=40)
        .map(|line| format!("original {line}\n"))
        .collect::<String>();
    std::fs::write(&path, &original).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Initial commit", "Baseline").unwrap();

    let modified = original
        .replace("original 3\n", "first edit\n")
        .replace("original 35\n", "second edit\n");
    std::fs::write(&path, &modified).unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot.commits[0].subject, "Initial commit");
    assert_eq!(snapshot.commits[0].author, "RitKraken Test");
    assert_eq!(
        snapshot.commits[0].author_email,
        "ritkraken-test@example.invalid"
    );
    assert_eq!(snapshot.refs.len(), 1);
    let target = FileTarget {
        scope: Scope::Worktree,
        change: snapshot.status.unstaged[0].clone(),
    };
    assert_eq!(target.change.path, Path::new(name));
    let patch = repo.patch(&target).unwrap();
    assert_eq!(patch.hunk_count(), 2);
    let full_patch = repo.full_patch(&target).unwrap().unwrap();
    assert!(patch.same_file_versions(&full_patch));
    assert!(String::from_utf8_lossy(&full_patch).contains(" original 20"));
    assert!(!String::from_utf8_lossy(patch.bytes()).contains(" original 20"));
    assert_eq!(repo.file_contents(&target).unwrap(), modified);
    repo.apply_partial(&target, &patch, PatchSelection::Hunk(0))
        .unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.staged[0].path, status.unstaged[0].path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), modified);

    let staged = FileTarget {
        scope: Scope::Index,
        change: status.staged[0].clone(),
    };
    let staged_patch = repo.patch(&staged).unwrap();
    let staged_patch_text = String::from_utf8_lossy(staged_patch.bytes());
    assert!(staged_patch_text.contains("+first edit"));
    assert!(!staged_patch_text.contains("+second edit"));
    let indexed = repo.file_contents(&staged).unwrap();
    assert!(indexed.contains("first edit"));
    assert!(!indexed.contains("second edit"));
    repo.apply_partial(&staged, &staged_patch, PatchSelection::Hunk(0))
        .unwrap();
    assert!(repo.status().unwrap().staged.is_empty());

    std::fs::write(&path, modified.replace("first edit", "changed externally")).unwrap();
    assert!(
        repo.apply_partial(&target, &patch, PatchSelection::Hunk(0))
            .unwrap_err()
            .to_string()
            .contains("changed on disk")
    );
    assert!(repo.status().unwrap().staged.is_empty());

    let current = repo.patch(&target).unwrap();
    repo.apply_partial(&target, &current, PatchSelection::Hunk(1))
        .unwrap();
    repo.commit("Partial commit", "").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot.commits[0].subject, "Partial commit");
    assert_eq!(
        snapshot.commits[0].parents,
        vec![snapshot.commits[1].id.clone()]
    );
    assert!(snapshot.status.staged.is_empty());
    assert_eq!(snapshot.status.unstaged.len(), 1);
    let detail = repo.detail(&snapshot.commits[0]).unwrap();
    assert_eq!(detail.changes[0].path, Path::new(name));
    assert!(detail.co_authors.is_empty());
    let committed = repo
        .file_contents(&FileTarget {
            scope: Scope::Commit {
                id: snapshot.commits[0].id.clone(),
                parent: snapshot.commits[0].parents.first().cloned(),
            },
            change: detail.changes[0].clone(),
        })
        .unwrap();
    assert!(committed.contains("second edit"));
    assert!(!committed.contains("changed externally"));
    assert_eq!(
        repo.committed_files(&snapshot.commits[0].id)
            .unwrap()
            .changes[0]
            .path,
        Path::new(name)
    );
    let limited = repo.snapshot(Some(1)).unwrap();
    assert!(limited.history_limited);
    assert_eq!(limited.commits[0].id, snapshot.commits[0].id);
    assert_eq!(limited.commits.len(), 1);
}

#[test]
fn commit_detail_lists_co_authors_and_the_committer() {
    let (_directory, repo) = fixture();
    let message = concat!(
        "Update the renderer\n\n",
        "Co-authored-by: Example in the body <sample@example.invalid>\n",
        "This paragraph documents the trailer syntax.\n\n",
        "Co-authored-by: 山田 太郎 <taro@example.invalid>\n",
        "co-authored-by: Duplicate <TARO@EXAMPLE.INVALID>\n",
        "Co-authored-by: Mina Author <author@example.invalid>\n",
        "Co-authored-by: Missing address\n",
        "Reviewed-by: Reviewer <reviewer@example.invalid>\n",
        "Co-authored-by: Alex\n",
        " Rossi <alex@example.invalid>\n",
    );
    git_output(
        command(Some(repo.root()))
            .env("GIT_COMMITTER_NAME", "Dana Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@example.invalid")
            .env("GIT_COMMITTER_DATE", "2001-06-10T12:00:00+00:00")
            .args([
                "commit",
                "--allow-empty",
                "--author=Mina Author <author@example.invalid>",
                "--date=2000-06-09T12:00:00+00:00",
                "--message",
                message,
            ]),
    );
    let snapshot = repo.snapshot(Some(1)).unwrap();
    let commit = &snapshot.commits[0];
    let detail = repo.detail(commit).unwrap();
    assert_eq!(commit.author, "Mina Author");
    assert!(commit.date.starts_with("2000/"));
    assert!(detail.committed_date.starts_with("2001/"));
    assert_eq!(detail.message, message.trim_end());
    assert_eq!(
        detail.committer,
        CommitPerson {
            name: "Dana Committer".into(),
            email: "committer@example.invalid".into(),
        }
    );
    assert_eq!(
        detail.co_authors,
        [
            CommitPerson {
                name: "山田 太郎".into(),
                email: "taro@example.invalid".into(),
            },
            CommitPerson {
                name: "Alex Rossi".into(),
                email: "alex@example.invalid".into(),
            },
        ]
    );
}

fn apply_line(repo: &Repo, scope: Scope, side: PatchSide, number: usize) {
    let status = repo.status().unwrap();
    let change = match &scope {
        Scope::Worktree => status.unstaged,
        Scope::Index => status.staged,
        Scope::Commit { .. } => unreachable!(),
    }
    .remove(0);
    let file = FileTarget { scope, change };
    let patch = repo.patch(&file).unwrap();
    repo.apply_partial(&file, &patch, PatchSelection::Line { side, number })
        .unwrap();
}

#[test]
fn a_line_action_targets_one_side_and_one_occurrence_of_repeated_edits() {
    let (_directory, repo) = fixture();
    let path = repo.root().join("repeated.txt");
    let first = format!("{}old\n{}", "context\n".repeat(5), "context\n".repeat(14));
    let tail = "context\n".repeat(7);
    let original = format!("{first}old\n{tail}");
    fs::write(&path, &original).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Baseline", "").unwrap();
    let modified = original.replace("old\n", "chosen\nother\n");
    fs::write(&path, &modified).unwrap();
    let indexed =
        || git_output(command(Some(repo.root())).args(["cat-file", "blob", ":repeated.txt"]));

    apply_line(&repo, Scope::Worktree, PatchSide::Modified, 22);
    assert_eq!(indexed(), format!("{first}old\nchosen\n{tail}").as_bytes());
    apply_line(&repo, Scope::Index, PatchSide::Modified, 22);
    assert_eq!(indexed(), original.as_bytes());

    apply_line(&repo, Scope::Worktree, PatchSide::Original, 21);
    assert_eq!(indexed(), format!("{first}{tail}").as_bytes());
    apply_line(&repo, Scope::Index, PatchSide::Original, 21);
    assert_eq!(indexed(), original.as_bytes());
    assert_eq!(fs::read(&path).unwrap(), modified.as_bytes());
}

#[test]
fn a_line_action_preserves_raw_bytes_at_the_end_of_the_file() {
    let (_directory, repo) = fixture();
    let path = repo.root().join("eof.txt");
    let indexed = || git_output(command(Some(repo.root())).args(["cat-file", "blob", ":eof.txt"]));
    fs::write(&path, b"keep\r\nold").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Baseline", "").unwrap();
    fs::write(&path, b"keep\r\nnew").unwrap();

    apply_line(&repo, Scope::Worktree, PatchSide::Modified, 2);
    assert_eq!(indexed(), b"keep\r\nold\r\nnew");
    apply_line(&repo, Scope::Index, PatchSide::Modified, 3);
    assert_eq!(indexed(), b"keep\r\nold\r\n");
    assert_eq!(fs::read(&path).unwrap(), b"keep\r\nnew");
}

#[test]
fn a_line_action_can_partially_stage_an_edit_of_a_tracked_empty_file() {
    let (_directory, repo) = fixture();
    let path = repo.root().join("empty.txt");
    let indexed =
        || git_output(command(Some(repo.root())).args(["cat-file", "blob", ":empty.txt"]));
    fs::write(&path, []).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Baseline", "").unwrap();
    fs::write(&path, b"first\r\nsecond\r\n").unwrap();

    apply_line(&repo, Scope::Worktree, PatchSide::Modified, 2);
    assert_eq!(indexed(), b"second\r\n");
    apply_line(&repo, Scope::Index, PatchSide::Modified, 1);
    assert!(indexed().is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"first\r\nsecond\r\n");
}

#[test]
fn whitespace_filtered_staging_uses_visible_context_and_preserves_index_bytes() {
    let (_directory, repo) = fixture();
    let path = repo.root().join("whitespace.txt");
    let original = "start\n  before\n  after\n  x = 1\nend\n";
    let modified = "start\nbefore  \nadded\n\tafter\n\tx  = 1\nend\n";
    fs::write(&path, original).unwrap();
    repo.stage(None).unwrap();
    repo.commit("Baseline", "").unwrap();
    fs::write(&path, modified).unwrap();

    let file = FileTarget {
        scope: Scope::Worktree,
        change: repo.status().unwrap().unstaged.remove(0),
    };
    let filtered = || {
        repo.patch(&file)
            .unwrap()
            .with_edge_whitespace_ignored(true)
    };
    let indexed =
        || git_output(command(Some(repo.root())).args(["cat-file", "blob", ":whitespace.txt"]));
    repo.apply_partial(
        &file,
        &filtered(),
        PatchSelection::Line {
            side: PatchSide::Modified,
            number: 3,
        },
    )
    .unwrap();
    assert_eq!(
        indexed(),
        b"start\n  before\nadded\n  after\n  x = 1\nend\n"
    );

    repo.apply_partial(&file, &filtered(), PatchSelection::Hunk(0))
        .unwrap();
    assert_eq!(
        indexed(),
        b"start\n  before\nadded\n  after\n\tx  = 1\nend\n"
    );
    let staged = FileTarget {
        scope: Scope::Index,
        change: repo.status().unwrap().staged.remove(0),
    };
    let patch = repo
        .patch(&staged)
        .unwrap()
        .with_edge_whitespace_ignored(true);
    repo.apply_partial(&staged, &patch, PatchSelection::Hunk(0))
        .unwrap();
    assert_eq!(indexed(), original.as_bytes());
    assert_eq!(fs::read(&path).unwrap(), modified.as_bytes());
}

#[test]
fn untracked_preview_and_unstage_work_on_an_unborn_branch() {
    let (_directory, repo) = fixture();
    std::fs::write(repo.root().join("new file.txt"), "new text\n").unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert!(snapshot.head.is_none());
    assert!(snapshot.commits.is_empty());
    let file = FileTarget {
        scope: Scope::Worktree,
        change: snapshot.status.unstaged[0].clone(),
    };
    let patch = repo.patch(&file).unwrap();
    assert!(String::from_utf8_lossy(patch.bytes()).contains("+new text"));
    repo.stage(Some(&file.change)).unwrap();
    let staged = repo.status().unwrap().staged.remove(0);
    repo.unstage(Some(&staged)).unwrap();
    assert_eq!(repo.status().unwrap().unstaged[0].code, b'?');
    assert!(repo.root().join("new file.txt").is_file());
}

#[test]
fn file_preview_enforces_the_same_limit_on_disk_and_in_the_index() {
    let (_directory, repo) = fixture();
    let path = repo.root().join("large.txt");
    let text = "a".repeat(PATCH_VIEW_LIMIT + 1);
    for size in [PATCH_VIEW_LIMIT, PATCH_VIEW_LIMIT + 1] {
        fs::write(&path, &text[..size]).unwrap();
        repo.stage(None).unwrap();
        let change = repo.status().unwrap().staged.remove(0);
        for scope in [Scope::Worktree, Scope::Index] {
            let target = FileTarget {
                scope,
                change: change.clone(),
            };
            let result = repo.file_contents(&target);
            if size == PATCH_VIEW_LIMIT {
                assert!(result.unwrap() == text[..size]);
            } else {
                assert!(result.unwrap_err().to_string().contains("preview limit"));
            }
        }
    }
}

#[test]
fn stash_round_trip_restores_the_index_and_untracked_files() {
    let (_directory, repo) = fixture();
    let tracked = repo.root().join("tracked.txt");
    let untracked = repo.root().join("untracked.txt");
    let ignored = repo.root().join("ignored.txt");
    fs::write(repo.root().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(&tracked, "base\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    fs::write(&tracked, "staged\n").unwrap();
    repo.stage(None).unwrap();
    fs::write(&tracked, "working tree\n").unwrap();
    fs::write(&untracked, "new file\n").unwrap();
    fs::write(&ignored, "ignored file\n").unwrap();

    repo.stash().unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert!(snapshot.has_stash);
    // The base commit and the stash row; Git's index and untracked helper
    // commits stay hidden.
    assert_eq!(snapshot.commits.len(), 2);
    assert_eq!(snapshot.commits[0].id, snapshot.stashes[0].id);
    assert_eq!(
        snapshot.commits[0].parents,
        [snapshot.commits[1].id.clone()]
    );
    assert!(snapshot.status.is_empty());
    assert_eq!(fs::read_to_string(&tracked).unwrap(), "base\n");
    assert!(!untracked.exists());
    assert!(ignored.is_file());

    repo.pop_stash().unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert!(!snapshot.has_stash);
    assert_eq!(snapshot.status.staged.len(), 1);
    assert_eq!(
        git_output(command(Some(repo.root())).args(["show", ":tracked.txt"])),
        b"staged\n"
    );
    assert_eq!(fs::read_to_string(&tracked).unwrap(), "working tree\n");
    assert_eq!(fs::read_to_string(&untracked).unwrap(), "new file\n");
}

#[test]
fn fetch_and_push_keep_the_checkout_and_target_branch_explicit() {
    let (_directory, repo) = fixture();
    let bare = tempfile::Builder::new()
        .prefix("ritkraken-remote-")
        .tempdir()
        .unwrap();
    git_output(
        command(None)
            .args(["init", "--bare", "--"])
            .arg(bare.path()),
    );
    git_output(
        command(Some(repo.root()))
            .args(["remote", "add", "origin"])
            .arg(bare.path()),
    );
    fs::write(repo.root().join("tracked.txt"), "base\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Base", "").unwrap();
    git_output(command(Some(repo.root())).args(["branch", "other"]));
    git_output(command(Some(repo.root())).args([
        "config",
        "remote.origin.push",
        "refs/heads/other",
    ]));
    let before = repo.snapshot(None).unwrap();
    assert_eq!(before.remotes, ["origin"]);
    assert!(before.upstream.is_none());

    let destination = Upstream {
        remote: "origin".into(),
        reference: "refs/heads/published".into(),
    };
    repo.push("main", &destination, true).unwrap();
    let tracked = repo.snapshot(None).unwrap();
    assert_eq!(tracked.upstream.as_ref(), Some(&destination));
    let remote_refs =
        git_output(command(Some(bare.path())).args(["for-each-ref", "--format=%(refname)"]));
    assert_eq!(
        std::str::from_utf8(&remote_refs)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["refs/heads/published"]
    );
    assert!(repo.push("other", &destination, false).is_err());

    fs::write(repo.root().join("tracked.txt"), "next revision\n").unwrap();
    repo.stage(None).unwrap();
    repo.commit("Next", "").unwrap();
    let local_head = repo.snapshot(None).unwrap().head.unwrap();
    repo.push("main", &destination, false).unwrap();
    for automatic in [false, true] {
        git_output(command(Some(repo.root())).args([
            "update-ref",
            "-d",
            "refs/remotes/origin/published",
        ]));
        if automatic {
            repo.auto_fetch().unwrap();
        } else {
            repo.fetch(Some("origin")).unwrap();
        }
        let fetched = repo.snapshot(None).unwrap();
        assert_eq!(fetched.head.as_ref(), Some(&local_head));
        assert!(
            fetched
                .refs
                .iter()
                .any(|reference| reference.name == "origin/published" && reference.id == local_head)
        );
    }
}

#[test]
fn snapshot_page_grows_commits_and_reuses_other_fields() {
    let (_directory, repo) = fixture();
    for index in 1..=5 {
        support::commits::commit_file(
            &repo,
            "file.txt",
            &format!("edit {index}"),
            &format!("Commit {index}"),
        );
    }
    let page = repo.snapshot(Some(2)).unwrap();
    assert_eq!(page.commits.len(), 2);
    assert!(page.history_limited);
    let grown = repo
        .snapshot_page(&page, Some(4), &HistoryFilter::default())
        .unwrap();
    assert_eq!(grown.commits.len(), 4);
    assert!(grown.history_limited);
    assert_eq!(grown.commits[..2], page.commits[..2]);
    assert_eq!(grown.status, page.status);
    assert_eq!(grown.refs, page.refs);
    let exhausted = repo
        .snapshot_page(&grown, Some(100), &HistoryFilter::default())
        .unwrap();
    assert_eq!(exhausted.commits.len(), 5);
    assert!(!exhausted.history_limited);
}
