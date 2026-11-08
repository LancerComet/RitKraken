mod support;

use ritkraken_git::{
    CommitAction as Action, CommitAvailability as Availability, CommitBlock, CommitOperation,
    CommitRelation, SequenceKind,
};
use std::fs;
use support::{commits::*, fixture};

#[test]
fn head_and_ancestor_menus_use_ancestry_beyond_the_loaded_graph() {
    let (_directory, repo) = fixture();
    let root = commit_file(&repo, "a", "one\n", "Root");
    commit_file(&repo, "a", "two\n", "Second");
    let tip = commit_file(&repo, "a", "three\n", "Third");
    let snapshot = repo.snapshot(Some(1)).unwrap();
    assert_eq!(snapshot.commits.len(), 1);
    let head = repo.commit_context(&tip).unwrap();
    assert_eq!(head.relation, CommitRelation::Head);
    assert_eq!(
        head.availability(Action::InteractiveRebase),
        Availability::Hidden
    );
    assert_eq!(head.availability(Action::Checkout), Availability::Hidden);
    assert_eq!(head.availability(Action::Reset), Availability::Hidden);
    assert_eq!(head.availability(Action::Amend), Availability::Enabled);
    assert_eq!(head.availability(Action::Revert), Availability::Enabled);
    let ancestor = repo.commit_context(&root).unwrap();
    assert_eq!(ancestor.relation, CommitRelation::Ancestor);
    assert_eq!(
        ancestor.availability(Action::InteractiveRebase),
        Availability::Enabled
    );
    assert_eq!(ancestor.availability(Action::Merge), Availability::Hidden);
    assert_eq!(ancestor.availability(Action::Rebase), Availability::Hidden);
    assert_eq!(repo.rebase_plan(&ancestor).unwrap().entries.len(), 2);
}

#[test]
fn diverged_descendant_unrelated_and_detached_states_have_different_actions() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let feature = commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    let descendant = repo.commit_context(&feature).unwrap();
    assert_eq!(descendant.relation, CommitRelation::Descendant);
    assert_eq!(
        descendant.availability(Action::Merge),
        Availability::Enabled
    );
    assert_eq!(
        descendant.availability(Action::Rebase),
        Availability::Hidden
    );
    commit_file(&repo, "main", "main", "Main");
    let diverged = repo.commit_context(&feature).unwrap();
    assert_eq!(diverged.relation, CommitRelation::Diverged);
    assert_eq!(diverged.availability(Action::Rebase), Availability::Enabled);
    assert_eq!(
        diverged.availability(Action::InteractiveRebase),
        Availability::Hidden
    );
    assert_eq!(diverged.availability(Action::Revert), Availability::Hidden);
    git(&repo, &["switch", "--detach", &base]);
    let detached = repo.commit_context(&feature).unwrap();
    assert_eq!(
        detached.availability(Action::CreateBranch),
        Availability::Enabled
    );
    for action in [
        Action::Merge,
        Action::Rebase,
        Action::CherryPick,
        Action::Reset,
    ] {
        assert_eq!(detached.availability(action), Availability::Hidden);
    }
    git(&repo, &["switch", "--orphan", "unrelated"]);
    let unrelated = commit_file(&repo, "other", "other", "Other root");
    git(&repo, &["switch", "main"]);
    let context = repo.commit_context(&unrelated).unwrap();
    assert_eq!(context.relation, CommitRelation::Unrelated);
    assert_eq!(context.availability(Action::Merge), Availability::Hidden);
    assert_eq!(context.availability(Action::Rebase), Availability::Hidden);
    assert_eq!(
        context.availability(Action::CherryPick),
        Availability::Enabled
    );
}

#[test]
fn dirty_worktree_disables_integration_but_keeps_branch_and_tag_creation() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "one", "Base");
    let tip = commit_file(&repo, "a", "two", "Tip");
    fs::write(repo.root().join("a"), "working").unwrap();
    let context = repo.commit_context(&base).unwrap();
    assert_eq!(
        context.availability(Action::InteractiveRebase),
        Availability::Disabled(CommitBlock::Dirty)
    );
    assert_eq!(
        context.availability(Action::CherryPick),
        Availability::Disabled(CommitBlock::Dirty)
    );
    assert_eq!(
        context.availability(Action::CreateBranch),
        Availability::Enabled
    );
    assert_eq!(
        context.availability(Action::CreateTag),
        Availability::Enabled
    );
    assert_eq!(
        repo.commit_context(&tip)
            .unwrap()
            .availability(Action::Amend),
        Availability::Enabled
    );
}

#[test]
fn merge_targets_and_merge_ranges_do_not_offer_linear_interactive_rebase() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    commit_file(&repo, "main", "main", "Main");
    git(&repo, &["merge", "--no-edit", "feature"]);
    let merge = head(&repo);
    commit_file(&repo, "later", "later", "Later");
    for id in [base, merge] {
        let context = repo.commit_context(&id).unwrap();
        assert_eq!(
            context.availability(Action::InteractiveRebase),
            Availability::Hidden
        );
    }
}

#[test]
fn conflict_state_offers_only_appropriate_sequence_controls() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "a", "base\n", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let target = commit_file(&repo, "a", "feature\n", "Feature");
    git(&repo, &["switch", "main"]);
    let current = commit_file(&repo, "a", "main\n", "Main");
    let context = repo.commit_context(&target).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, CommitOperation::CherryPick { mainline: None })
        .unwrap();
    assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
    let conflict = repo.commit_context(&current).unwrap();
    assert_eq!(conflict.sequence, Some(SequenceKind::CherryPick));
    assert_eq!(
        conflict.availability(Action::Continue),
        Availability::Disabled(CommitBlock::Conflict)
    );
    assert_eq!(conflict.availability(Action::Abort), Availability::Enabled);
    assert_eq!(conflict.availability(Action::Skip), Availability::Enabled);
    assert_eq!(
        conflict.availability(Action::CreateBranch),
        Availability::Disabled(CommitBlock::InProgress)
    );
    apply(&repo, &current, CommitOperation::Abort);
    assert!(repo.sequence_kind().unwrap().is_none());
    assert_eq!(fs::read_to_string(repo.root().join("a")).unwrap(), "main\n");
}

#[test]
fn checkout_choices_deduplicate_tracking_refs_and_preserve_nested_branch_names() {
    let (_directory, repo) = fixture();
    let tip = commit_file(&repo, "base", "base", "Base");
    git(
        &repo,
        &["remote", "add", "origin", repo.root().to_str().unwrap()],
    );
    git(&repo, &["update-ref", "refs/remotes/origin/main", &tip]);
    git(&repo, &["branch", "--set-upstream-to=origin/main", "main"]);
    let context = repo.commit_context(&tip).unwrap();
    assert_eq!(context.availability(Action::Switch), Availability::Hidden);
    git(&repo, &["branch", "topic/nested", &tip]);
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/topic/nested", &tip],
    );
    let context = repo.commit_context(&tip).unwrap();
    let choices: Vec<_> = context
        .checkout_references()
        .map(|reference| reference.name.as_str())
        .collect();
    assert_eq!(choices, vec!["topic/nested"]);
    assert_eq!(
        context.remote_branch_name("origin/topic/nested"),
        "topic/nested"
    );
}
