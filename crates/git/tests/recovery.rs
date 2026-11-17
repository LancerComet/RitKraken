mod support;

use std::fs;

use ritkraken_git::{Error, RefKind, RefOperation, RefRequest, RuleError};
use support::commits::{commit_file, git, head};
use support::{command, fixture, git_output};

#[test]
fn updates_without_messages_remain_available_for_reverse_recovery() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
    let next = git(
        &repo,
        &["commit-tree", tree.trim(), "-p", &base, "-m", "Next"],
    );
    let next = next.trim();
    git_output(
        command(Some(repo.root()))
            .env("GIT_COMMITTER_DATE", "2024-02-29T23:59:59+0530")
            .args(["update-ref", "refs/heads/main", next]),
    );

    let history = repo.recovery_history().unwrap();
    assert_eq!(history.entries[0].id, next);
    assert_eq!(history.entries[0].date, "2024/02/29 @ 23:59 +0530");
    repo.restore_head(&history, &base).unwrap();
    assert_eq!(
        repo.recovery_history().unwrap().undo_target.as_deref(),
        Some(next)
    );
}

#[test]
fn external_rewrites_can_restore_original_head_without_touching_any_file_bytes() {
    for operation in ["amend", "rebase", "reset"] {
        let (_directory, repo) = fixture();
        let base = commit_file(&repo, "base", "base\n", "Base");
        git(&repo, &["branch", "upstream"]);
        let original = commit_file(&repo, "tracked", "original\n", "Original");
        match operation {
            "amend" => {
                git(&repo, &["commit", "--amend", "-m", "Amended"]);
            }
            "rebase" => {
                git(&repo, &["switch", "upstream"]);
                commit_file(&repo, "upstream", "upstream\n", "Upstream");
                git(&repo, &["switch", "main"]);
                git(&repo, &["rebase", "upstream"]);
            }
            "reset" => {
                git(&repo, &["reset", "--hard", &base]);
            }
            _ => unreachable!(),
        }
        let displaced = head(&repo);
        assert_ne!(displaced, original);
        fs::write(repo.root().join("tracked"), b"staged\0bytes\r\n").unwrap();
        git(&repo, &["add", "tracked"]);
        fs::write(repo.root().join("tracked"), b"working\0bytes\n").unwrap();
        fs::write(repo.root().join("untracked"), b"untracked\xff\r\n").unwrap();
        let index_path = repo.root().join(".git/index");
        let index = fs::read(&index_path).unwrap();
        let history = repo.recovery_history().unwrap();
        assert!(history.entries.iter().any(|entry| entry.id == original));

        repo.restore_head(&history, &original).unwrap();

        assert_eq!(head(&repo), original, "{operation}");
        assert_eq!(fs::read(index_path).unwrap(), index, "{operation}");
        assert_eq!(
            fs::read(repo.root().join("tracked")).unwrap(),
            b"working\0bytes\n",
            "{operation}"
        );
        assert_eq!(
            git_output(command(Some(repo.root())).args(["show", ":tracked"])),
            b"staged\0bytes\r\n",
            "{operation}"
        );
        assert_eq!(
            fs::read(repo.root().join("untracked")).unwrap(),
            b"untracked\xff\r\n",
            "{operation}"
        );
        assert_eq!(
            repo.recovery_history().unwrap().undo_target,
            Some(displaced),
            "{operation}"
        );
    }
}

#[test]
fn undo_uses_previous_distinct_head_and_does_not_reverse_checkout() {
    let (_directory, repo) = fixture();
    let previous = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["branch", "previous"]);
    let current = commit_file(&repo, "next", "next", "Next");
    git(&repo, &["switch", "-c", "same"]);
    let history = repo.recovery_history().unwrap();
    assert_eq!(history.undo_target.as_deref(), Some(previous.as_str()));
    repo.restore_head(&history, &previous).unwrap();
    assert_eq!(
        git(&repo, &["symbolic-ref", "--short", "HEAD"]).trim(),
        "same"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]).trim(), current);
}

#[test]
fn stale_checkout_is_rejected_even_when_head_oid_matches() {
    let (_directory, repo) = fixture();
    let target = commit_file(&repo, "base", "base", "Base");
    let history = repo.recovery_history().unwrap();
    git(&repo, &["switch", "-c", "other"]);
    for result in [
        repo.restore_head(&history, &target),
        repo.recover_branch(&history, &target, "recovered"),
    ] {
        assert!(matches!(result, Err(Error::Rule(RuleError::StaleCommit))));
    }
    assert_eq!(head(&repo), target);
}

#[test]
fn recovery_rejects_sequences_and_unmerged_index_without_changing_head() {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
        "rebase-merge",
        "rebase-apply",
        "sequencer/todo",
        "conflicts",
    ] {
        let (_directory, repo) = fixture();
        let target = commit_file(&repo, "base", "base", "Base");
        if marker == "conflicts" {
            let blob = git(&repo, &["rev-parse", "HEAD:base"]);
            let blob = blob.trim();
            let input = format!("100644 {blob} 1\tconflicted\n100644 {blob} 2\tconflicted\n");
            use std::io::Write;
            use std::process::Stdio;
            let mut child = command(Some(repo.root()))
                .args(["update-index", "--index-info"])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
        } else {
            let path = repo.root().join(".git").join(marker);
            if marker.starts_with("rebase") {
                fs::create_dir(path).unwrap();
            } else {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, &target).unwrap();
            }
        }
        let history = repo.recovery_history().unwrap();
        for result in [
            repo.restore_head(&history, &target),
            repo.recover_branch(&history, &target, "recovered"),
        ] {
            let expected = if marker == "conflicts" {
                matches!(result, Err(Error::Rule(RuleError::UnresolvedConflicts)))
            } else {
                matches!(result, Err(Error::Rule(RuleError::SequenceInProgress)))
            };
            assert!(expected, "{marker}");
        }
        assert_eq!(head(&repo), target);
    }
}

#[test]
fn deleted_branch_tip_survives_reflog_removal_gc_and_reopens_as_new_branch() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    // A commit never visited by HEAD cannot be recovered from HEAD's reflog.
    let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
    let tip = git(
        &repo,
        &["commit-tree", tree.trim(), "-p", &base, "-m", "Lost"],
    );
    let tip = tip.trim();
    git(&repo, &["branch", "lost/topic", tip]);
    // Reference deletion and recovery must not require a configured author.
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    git(&repo, &["config", "user.useConfigOnly", "true"]);
    let snapshot = repo.snapshot(None).unwrap();
    let reference = snapshot
        .refs
        .iter()
        .find(|reference| reference.kind == RefKind::Local && reference.name == "lost/topic")
        .unwrap()
        .clone();
    repo.apply_ref_operation(&RefRequest::new(&snapshot, reference, RefOperation::Delete))
        .unwrap();
    assert!(!repo.root().join(".git/logs/refs/heads/lost/topic").exists());
    git(&repo, &["gc", "--prune=now"]);
    let history = repo.recovery_history().unwrap();
    let entry = history
        .entries
        .iter()
        .find(|entry| entry.id == tip)
        .unwrap();
    assert_eq!(entry.message, "deleted branch: lost/topic");
    assert!(
        repo.snapshot(None)
            .unwrap()
            .commits
            .iter()
            .all(|commit| commit.id != tip)
    );

    repo.recover_branch(&history, tip, "recovered/topic")
        .unwrap();

    assert_eq!(git(&repo, &["rev-parse", "recovered/topic"]).trim(), tip);
    assert_eq!(head(&repo), base);
    assert!(matches!(
        repo.recover_branch(&history, tip, "recovered/topic"),
        Err(Error::Rule(RuleError::RefExists))
    ));
}

#[test]
fn recovery_requires_an_exact_history_commit_and_handles_empty_repository() {
    let (_directory, repo) = fixture();
    let empty = repo.recovery_history().unwrap();
    assert_eq!(empty.head, None);
    assert_eq!(empty.branch.as_deref(), Some("main"));
    assert!(empty.entries.is_empty());
    assert_eq!(empty.undo_target, None);
    assert!(matches!(
        repo.restore_head(&empty, "HEAD"),
        Err(Error::Rule(RuleError::InvalidCommit))
    ));
    let target = commit_file(&repo, "base", "base", "Base");
    let history = repo.recovery_history().unwrap();
    let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
    let unseen = git(&repo, &["commit-tree", tree.trim(), "-m", "Unseen"]);
    for invalid in ["HEAD", &target[..12], unseen.trim(), "--hard"] {
        assert!(matches!(
            repo.restore_head(&history, invalid),
            Err(Error::Rule(RuleError::InvalidCommit))
        ));
    }
    for invalid in ["HEAD", "-branch", "bad..name"] {
        assert!(matches!(
            repo.recover_branch(&history, &target, invalid),
            Err(Error::Rule(RuleError::InvalidRefName))
        ));
    }
    assert_eq!(head(&repo), target);
}
