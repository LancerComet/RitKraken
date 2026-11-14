mod support;

use ritkraken_git::{
    CommitOperation as Op, ConflictResolution as Choice, Error, FileTarget, Repo, RuleError, Scope,
};
use std::fs;
use support::{commits::*, fixture};

fn start(repo: &Repo, bytes: bool) -> String {
    commit_file(repo, "test", "test", "Base");
    git(repo, &["switch", "-c", "incoming"]);
    fs::write(
        repo.root().join("test"),
        if bytes {
            b"\0incoming".as_slice()
        } else {
            b"test\nupdate\nJB"
        },
    )
    .unwrap();
    git(repo, &["add", "test"]);
    repo.commit("Incoming", "").unwrap();
    let target = head(repo);
    git(repo, &["switch", "main"]);
    fs::write(
        repo.root().join("test"),
        if bytes {
            b"\0current".as_slice()
        } else {
            b"test\nlocal"
        },
    )
    .unwrap();
    git(repo, &["add", "test"]);
    repo.commit("Current", "").unwrap();
    let context = repo.commit_context(&target).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::CherryPick { mainline: None })
        .unwrap();
    assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
    head(repo)
}

fn file(repo: &Repo) -> FileTarget {
    let change = repo
        .status()
        .unwrap()
        .unstaged
        .into_iter()
        .find(|file| file.code == b'U')
        .unwrap();
    FileTarget {
        scope: Scope::Worktree,
        change,
    }
}

#[test]
fn unresolved_markers_cannot_be_staged_or_marked_resolved() {
    let (_directory, repo) = fixture();
    start(&repo, false);
    let target = file(&repo);
    let expected = repo.conflict_file(&target).unwrap().unwrap();
    for result in [
        repo.stage(Some(&target.change)),
        repo.stage(None),
        repo.resolve_conflict(&expected, &Choice::WorkingFile),
    ] {
        assert!(matches!(
            result,
            Err(Error::Rule(RuleError::UnresolvedConflicts))
        ));
    }
    git(&repo, &["add", "test"]);
    let context = repo.commit_context(&head(&repo)).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::Continue)
        .unwrap();
    assert!(matches!(
        repo.apply_commit_operation(&prepared, &editor()),
        Err(Error::Rule(RuleError::UnresolvedConflicts))
    ));
    assert!(repo.sequence_kind().unwrap().is_some());
}

#[test]
fn reviewed_versions_and_manual_output_resolve_without_a_commit() {
    for choice in [
        Choice::Current,
        Choice::Incoming,
        Choice::Text("手动合并\r\n".into()),
    ] {
        let (_directory, repo) = fixture();
        let original = start(&repo, false);
        let target = file(&repo);
        let expected = repo.conflict_file(&target).unwrap().unwrap();
        let text = match &choice {
            Choice::Current => "test\nlocal",
            Choice::Incoming => "test\nupdate\nJB",
            _ => "手动合并\r\n",
        };
        repo.resolve_conflict(&expected, &choice).unwrap();
        assert_eq!(fs::read_to_string(repo.root().join("test")).unwrap(), text);
        assert!(!repo.status().unwrap().has_conflicts());
        apply(&repo, &original, Op::Continue);
        assert_eq!(head(&repo), original);
        assert!(repo.sequence_kind().unwrap().is_none());
    }
}

#[test]
fn externally_changed_content_is_never_overwritten_by_a_stale_resolution() {
    let (_directory, repo) = fixture();
    start(&repo, false);
    let expected = repo.conflict_file(&file(&repo)).unwrap().unwrap();
    fs::write(repo.root().join("test"), "external edit").unwrap();
    assert!(matches!(
        repo.resolve_conflict(&expected, &Choice::Incoming),
        Err(Error::Rule(RuleError::StaleFile))
    ));
    assert_eq!(
        fs::read_to_string(repo.root().join("test")).unwrap(),
        "external edit"
    );
}

#[test]
fn cancellation_restores_exact_pre_pick_content_after_partial_resolution() {
    for resolved in [false, true] {
        let (_directory, repo) = fixture();
        let original = start(&repo, false);
        if resolved {
            let expected = repo.conflict_file(&file(&repo)).unwrap().unwrap();
            repo.resolve_conflict(&expected, &Choice::Incoming).unwrap();
        }
        apply(&repo, &original, Op::Abort);
        assert_eq!(head(&repo), original);
        assert_eq!(
            fs::read_to_string(repo.root().join("test")).unwrap(),
            "test\nlocal"
        );
        assert!(repo.status().unwrap().is_empty());
        assert!(repo.sequence_kind().unwrap().is_none());
    }
}

#[test]
fn binary_conflicts_can_take_either_complete_version() {
    for choice in [Choice::Current, Choice::Incoming] {
        let (_directory, repo) = fixture();
        start(&repo, true);
        let expected = repo.conflict_file(&file(&repo)).unwrap().unwrap();
        assert!(expected.working_text.is_none());
        repo.resolve_conflict(&expected, &choice).unwrap();
        assert_eq!(
            fs::read(repo.root().join("test")).unwrap(),
            if matches!(choice, Choice::Current) {
                b"\0current".as_slice()
            } else {
                b"\0incoming".as_slice()
            }
        );
        assert!(!repo.status().unwrap().has_conflicts());
    }
}

#[test]
fn modify_delete_conflicts_offer_both_deletion_and_retaining_content() {
    for incoming_deletes in [false, true] {
        for choice in [Choice::Current, Choice::Incoming] {
            let (_directory, repo) = fixture();
            commit_file(&repo, "test", "base", "Base");
            git(&repo, &["switch", "-c", "incoming"]);
            if incoming_deletes {
                git(&repo, &["rm", "test"]);
                repo.commit("Delete", "").unwrap();
            } else {
                commit_file(&repo, "test", "incoming", "Incoming");
            }
            let target = head(&repo);
            git(&repo, &["switch", "main"]);
            if incoming_deletes {
                commit_file(&repo, "test", "current", "Current");
            } else {
                git(&repo, &["rm", "test"]);
                repo.commit("Delete", "").unwrap();
            }
            let original = head(&repo);
            let context = repo.commit_context(&target).unwrap();
            let prepared = repo
                .prepare_commit_operation(&context, Op::CherryPick { mainline: None })
                .unwrap();
            assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
            let expected = repo.conflict_file(&file(&repo)).unwrap().unwrap();
            repo.resolve_conflict(&expected, &choice).unwrap();
            let deleted = incoming_deletes == matches!(choice, Choice::Incoming);
            assert_eq!(repo.root().join("test").exists(), !deleted);
            assert!(!repo.status().unwrap().has_conflicts());
            assert_eq!(head(&repo), original);
        }
    }
}

#[test]
fn leftover_markers_without_an_active_sequence_can_restore_the_current_version() {
    let (_directory, repo) = fixture();
    let original = start(&repo, false);
    let markers = fs::read_to_string(repo.root().join("test")).unwrap();
    apply(&repo, &original, Op::Abort);
    fs::write(repo.root().join("test"), markers).unwrap();
    let change = repo.status().unwrap().unstaged.remove(0);
    assert_eq!(change.code, b'M');
    let expected = repo
        .conflict_file(&FileTarget {
            scope: Scope::Worktree,
            change,
        })
        .unwrap()
        .unwrap();
    assert!(!expected.unmerged);
    repo.resolve_conflict(&expected, &Choice::Current).unwrap();
    assert_eq!(
        fs::read_to_string(repo.root().join("test")).unwrap(),
        "test\nlocal"
    );
    assert_eq!(head(&repo), original);
    assert!(repo.status().unwrap().is_empty());
}

#[test]
fn marking_a_large_working_file_checks_for_markers_beyond_the_preview_limit() {
    let (_directory, repo) = fixture();
    start(&repo, false);
    let mut text = "x".repeat(ritkraken_git::PATCH_VIEW_LIMIT + 1);
    text.push_str("\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> source\n");
    fs::write(repo.root().join("test"), text).unwrap();
    let expected = repo.conflict_file(&file(&repo)).unwrap().unwrap();
    assert!(expected.working_text.is_none());
    assert!(matches!(
        repo.resolve_conflict(&expected, &Choice::WorkingFile),
        Err(Error::Rule(RuleError::UnresolvedConflicts))
    ));
}
