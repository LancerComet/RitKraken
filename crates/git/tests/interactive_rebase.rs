mod support;

use ritkraken_git::{
    CommitAction, CommitAvailability, CommitOperation as Op, Error, RebaseAction, RuleError,
    SequenceKind,
};
use std::fs;
use support::{commits::*, fixture};

#[test]
fn interactive_rebase_rewords_squashes_drops_and_preserves_the_base_commit() {
    let (directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    let first = commit_file(&repo, "one", "one", "First");
    commit_file(&repo, "two", "two", "Second");
    commit_file(&repo, "drop", "drop", "Dropped");
    let context = repo.commit_context(&base).unwrap();
    let mut plan = repo.rebase_plan(&context).unwrap();
    assert_eq!(plan.entries[0].commit.id, first);
    plan.entries[0].action = RebaseAction::Reword;
    plan.entries[0].message = "Renamed\n\nUnicode: 中文 日本語".into();
    plan.entries[1].action = RebaseAction::Squash;
    plan.entries[2].action = RebaseAction::Drop;
    let prepared = repo
        .prepare_commit_operation(&context, Op::InteractiveRebase(plan))
        .unwrap();
    repo.apply_commit_operation(&prepared, &reword_editor(&directory.path().join(".git")))
        .unwrap();
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(git(&repo, &["rev-parse", "HEAD^"]).trim(), base);
    let message = repo.commit_context(&head(&repo)).unwrap().message;
    assert!(
        message.contains("Renamed\n\nUnicode: 中文 日本語"),
        "{message}"
    );
    assert!(message.contains("Second"), "{message}");
    assert!(!repo.root().join("drop").exists());
    assert!(repo.root().join("one").exists());
    assert!(repo.root().join("two").exists());
    assert!(repo.sequence_kind().unwrap().is_none());
}

#[test]
fn reordering_independent_commits_and_fixup_keeps_the_selected_message() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    commit_file(&repo, "one", "one", "First");
    commit_file(&repo, "two", "two", "Second");
    let context = repo.commit_context(&base).unwrap();
    let mut plan = repo.rebase_plan(&context).unwrap();
    plan.entries.swap(0, 1);
    plan.entries[1].action = RebaseAction::Fixup;
    let prepared = repo
        .prepare_commit_operation(&context, Op::InteractiveRebase(plan))
        .unwrap();
    repo.apply_commit_operation(&prepared, &editor()).unwrap();
    assert_eq!(repo.commit_context(&head(&repo)).unwrap().message, "Second");
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "2");
    assert!(repo.root().join("one").exists() && repo.root().join("two").exists());
}

#[test]
fn invalid_plans_and_head_anchored_rebases_are_rejected_without_modifying_history() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    commit_file(&repo, "one", "one", "First");
    let tip = commit_file(&repo, "two", "two", "Second");
    let context = repo.commit_context(&base).unwrap();
    let mut plan = repo.rebase_plan(&context).unwrap();
    plan.entries[0].action = RebaseAction::Squash;
    assert!(matches!(
        repo.prepare_commit_operation(&context, Op::InteractiveRebase(plan)),
        Err(Error::Rule(RuleError::InvalidRebasePlan))
    ));
    let mut plan = repo.rebase_plan(&context).unwrap();
    plan.entries.pop();
    assert!(
        repo.prepare_commit_operation(&context, Op::InteractiveRebase(plan))
            .is_err()
    );
    let mut plan = repo.rebase_plan(&context).unwrap();
    plan.entries[1] = plan.entries[0].clone();
    assert!(
        repo.prepare_commit_operation(&context, Op::InteractiveRebase(plan))
            .is_err()
    );
    let head_context = repo.commit_context(&tip).unwrap();
    assert!(repo.rebase_plan(&head_context).is_err());
    assert_eq!(head(&repo), tip);
}

#[test]
fn edit_pause_allows_amend_and_continue_with_the_new_head_visible_in_the_graph() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    commit_file(&repo, "one", "one", "First");
    commit_file(&repo, "two", "two", "Second");
    let context = repo.commit_context(&base).unwrap();
    let mut plan = repo.rebase_plan(&context).unwrap();
    plan.entries[0].action = RebaseAction::Edit;
    let prepared = repo
        .prepare_commit_operation(&context, Op::InteractiveRebase(plan))
        .unwrap();
    repo.apply_commit_operation(&prepared, &editor()).unwrap();
    assert_eq!(repo.sequence_kind().unwrap(), Some(SequenceKind::Rebase));
    let paused = head(&repo);
    let context = repo.commit_context(&paused).unwrap();
    assert!(context.editing_rebase_commit);
    assert_eq!(
        context.availability(CommitAction::Amend),
        CommitAvailability::Enabled
    );
    apply(
        &repo,
        &paused,
        Op::Amend {
            message: "Edited message".into(),
            include_staged: false,
        },
    );
    let amended = head(&repo);
    assert_ne!(amended, paused);
    let snapshot = repo.snapshot(None).unwrap();
    assert!(snapshot.commits.iter().any(|commit| commit.id == amended));
    apply(&repo, &amended, Op::Continue);
    assert!(repo.sequence_kind().unwrap().is_none());
    assert_eq!(
        git(&repo, &["log", "-2", "--format=%s"]),
        "Second\nEdited message\n"
    );
}

#[test]
fn rebase_conflicts_can_be_aborted_without_moving_the_original_branch() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a", "base\n", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let feature = commit_file(&repo, "a", "feature\n", "Feature");
    git(&repo, &["switch", "main"]);
    let main = commit_file(&repo, "a", "main\n", "Main");
    let context = repo.commit_context(&feature).unwrap();
    let prepared = repo.prepare_commit_operation(&context, Op::Rebase).unwrap();
    assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
    assert_eq!(repo.sequence_kind().unwrap(), Some(SequenceKind::Rebase));
    apply(&repo, &head(&repo), Op::Abort);
    assert_eq!(head(&repo), main);
    assert_eq!(fs::read_to_string(repo.root().join("a")).unwrap(), "main\n");
}

#[test]
fn cherry_pick_conflicts_support_continue_after_resolution_and_skip() {
    for skip in [false, true] {
        let (_directory, repo) = fixture();
        commit_file(&repo, "a", "base\n", "Base");
        git(&repo, &["switch", "-c", "feature"]);
        let feature = commit_file(&repo, "a", "feature\n", "Feature");
        git(&repo, &["switch", "main"]);
        let main = commit_file(&repo, "a", "main\n", "Main");
        let context = repo.commit_context(&feature).unwrap();
        let prepared = repo
            .prepare_commit_operation(&context, Op::CherryPick { mainline: None })
            .unwrap();
        assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
        if skip {
            apply(&repo, &main, Op::Skip);
            assert_eq!(head(&repo), main);
        } else {
            fs::write(repo.root().join("a"), "resolved\n").unwrap();
            git(&repo, &["add", "a"]);
            apply(&repo, &main, Op::Continue);
            assert_eq!(
                fs::read_to_string(repo.root().join("a")).unwrap(),
                "resolved\n"
            );
            assert_ne!(head(&repo), main);
        }
        assert!(repo.sequence_kind().unwrap().is_none());
    }
}
