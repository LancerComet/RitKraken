mod support;

use ritkraken_git::{CommitOperation as Op, Error, ResetMode, RuleError};
use std::fs;
use support::{commits::*, fixture};

#[test]
fn creates_refs_at_the_clicked_commit_and_tracks_remote_branches_explicitly() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "base\n", "Base");
    let tip = commit_file(&repo, "a", "tip\n", "Tip");
    apply(
        &repo,
        &base,
        Op::CreateBranch {
            name: "old-point".into(),
            checkout: false,
            track: None,
        },
    );
    assert_eq!(git(&repo, &["rev-parse", "old-point"]).trim(), base);
    assert_eq!(head(&repo), tip);
    apply(&repo, &base, Op::CreateTag("v1".into()));
    assert_eq!(git(&repo, &["rev-parse", "v1"]).trim(), base);
    git(
        &repo,
        &["remote", "add", "origin", repo.root().to_str().unwrap()],
    );
    git(&repo, &["update-ref", "refs/remotes/origin/topic", &base]);
    apply(
        &repo,
        &base,
        Op::CreateBranch {
            name: "topic".into(),
            checkout: true,
            track: Some("origin/topic".into()),
        },
    );
    assert_eq!(head(&repo), base);
    assert_eq!(
        git(&repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(),
        "origin/topic"
    );
    apply(&repo, &tip, Op::Switch("main".into()));
    assert_eq!(head(&repo), tip);
    apply(&repo, &base, Op::Checkout);
    assert!(repo.snapshot(None).unwrap().branch.is_none());
}

#[test]
fn changed_heads_or_moved_branch_targets_cannot_execute_stale_menu_actions() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "base", "Base");
    let tip = commit_file(&repo, "a", "tip", "Tip");
    git(&repo, &["branch", "other", &base]);
    let context = repo.commit_context(&base).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::Switch("other".into()))
        .unwrap();
    git(&repo, &["branch", "-f", "other", &tip]);
    assert!(matches!(
        repo.apply_commit_operation(&prepared, &editor()),
        Err(Error::Rule(RuleError::StaleCommit))
    ));
    commit_file(&repo, "a", "external", "External");
    assert!(matches!(
        repo.prepare_commit_operation(&context, Op::Reset(ResetMode::Soft)),
        Err(Error::Rule(RuleError::StaleCommit))
    ));
}

#[test]
fn reset_modes_preserve_exactly_the_documented_index_and_worktree_versions() {
    for mode in [ResetMode::Soft, ResetMode::Mixed, ResetMode::Hard] {
        let (_directory, repo) = fixture();
        let base = commit_file(&repo, "a", "base\n", "Base");
        commit_file(&repo, "a", "tip\n", "Tip");
        fs::write(repo.root().join("a"), "staged\n").unwrap();
        git(&repo, &["add", "a"]);
        fs::write(repo.root().join("a"), "working\n").unwrap();
        fs::write(repo.root().join("untracked"), "keep me").unwrap();
        apply(&repo, &base, Op::Reset(mode));
        assert_eq!(head(&repo), base);
        assert_eq!(
            git(&repo, &["show", ":a"]),
            if mode == ResetMode::Soft {
                "staged\n"
            } else {
                "base\n"
            }
        );
        assert_eq!(
            fs::read_to_string(repo.root().join("a")).unwrap(),
            if mode == ResetMode::Hard {
                "base\n"
            } else {
                "working\n"
            }
        );
        assert_eq!(
            fs::read_to_string(repo.root().join("untracked")).unwrap(),
            "keep me"
        );
    }
}

#[test]
fn destructive_reset_refuses_content_changes_after_the_confirmation_snapshot() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "base", "Base");
    commit_file(&repo, "a", "tip", "Tip");
    fs::write(repo.root().join("a"), "before").unwrap();
    let context = repo.commit_context(&base).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::Reset(ResetMode::Hard))
        .unwrap();
    fs::write(repo.root().join("a"), "after!").unwrap();
    assert!(matches!(
        repo.apply_commit_operation(&prepared, &editor()),
        Err(Error::Rule(RuleError::StaleCommit))
    ));
    assert_eq!(fs::read_to_string(repo.root().join("a")).unwrap(), "after!");
}

#[test]
fn amend_message_only_preserves_staged_and_unstaged_contents() {
    let (_directory, repo) = fixture();
    let old = commit_file(&repo, "a", "committed\n", "Original");
    fs::write(repo.root().join("a"), "staged\n").unwrap();
    git(&repo, &["add", "a"]);
    fs::write(repo.root().join("a"), "working\n").unwrap();
    apply(
        &repo,
        &old,
        Op::Amend {
            message: "New message\n\nBody".into(),
            include_staged: false,
        },
    );
    assert_eq!(git(&repo, &["show", "HEAD:a"]), "committed\n");
    assert_eq!(git(&repo, &["show", ":a"]), "staged\n");
    assert_eq!(
        fs::read_to_string(repo.root().join("a")).unwrap(),
        "working\n"
    );
    assert_eq!(
        repo.commit_context(&head(&repo)).unwrap().message,
        "New message\n\nBody"
    );
    apply(
        &repo,
        &head(&repo),
        Op::Amend {
            message: "Include staged".into(),
            include_staged: true,
        },
    );
    assert_eq!(git(&repo, &["show", "HEAD:a"]), "staged\n");
    assert_eq!(
        fs::read_to_string(repo.root().join("a")).unwrap(),
        "working\n"
    );
}

#[test]
fn cherry_pick_revert_and_fast_forward_apply_to_the_current_branch() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let feature = commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    apply(&repo, &feature, Op::Merge);
    assert_eq!(head(&repo), feature);
    apply(&repo, &feature, Op::Revert { mainline: None });
    assert!(!repo.root().join("feature").exists());
    // An ancestor can be useful again after a revert: do not hide cherry-pick
    // merely because the original commit is reachable from HEAD.
    apply(&repo, &feature, Op::CherryPick { mainline: None });
    assert_eq!(
        fs::read_to_string(repo.root().join("feature")).unwrap(),
        "feature"
    );
    assert_ne!(head(&repo), base);
}

#[test]
fn merge_commit_revert_requires_an_explicit_valid_parent() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "base", "base", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    commit_file(&repo, "feature", "feature", "Feature");
    git(&repo, &["switch", "main"]);
    commit_file(&repo, "main", "main", "Main");
    git(&repo, &["merge", "--no-edit", "feature"]);
    let merge = head(&repo);
    let context = repo.commit_context(&merge).unwrap();
    assert!(matches!(
        repo.prepare_commit_operation(&context, Op::Revert { mainline: None }),
        Err(Error::Rule(RuleError::MainlineRequired))
    ));
    assert!(
        repo.prepare_commit_operation(&context, Op::Revert { mainline: Some(3) })
            .is_err()
    );
    apply(&repo, &merge, Op::Revert { mainline: Some(1) });
    assert!(!repo.root().join("feature").exists());
    assert!(repo.root().join("main").exists());
}

#[test]
fn published_detection_covers_pushed_ancestors_below_an_unpushed_head() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "base", "Base");
    let published = commit_file(&repo, "a", "pushed", "Pushed");
    let tip = commit_file(&repo, "a", "local", "Local");
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/main", &published],
    );
    let context = repo.commit_context(&base).unwrap();
    let plan = repo.rebase_plan(&context).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::InteractiveRebase(plan))
        .unwrap();
    assert_eq!(prepared.affected_count, 2);
    assert_eq!(prepared.published, vec!["origin/main"]);
    let tip_context = repo.commit_context(&tip).unwrap();
    assert!(
        repo.prepare_commit_operation(
            &tip_context,
            Op::Amend {
                message: "Local only".into(),
                include_staged: false
            }
        )
        .unwrap()
        .published
        .is_empty()
    );
    let published_context = repo.commit_context(&published).unwrap();
    assert!(
        repo.prepare_commit_operation(&published_context, Op::Reset(ResetMode::Soft))
            .unwrap()
            .published
            .is_empty()
    );
}

#[test]
fn root_and_merge_exports_produce_usable_patches_without_truncating_failed_exports() {
    let (directory, repo) = fixture();
    let base = commit_file(&repo, "a", "root contents\n", "Root");
    let patch = directory.path().join("root.patch");
    repo.export_commit_patch(&base, None, &patch).unwrap();
    let (_other_dir, other) = fixture();
    git(&other, &["apply", "--check", patch.to_str().unwrap()]);
    git(&other, &["apply", patch.to_str().unwrap()]);
    assert_eq!(
        fs::read_to_string(other.root().join("a")).unwrap(),
        "root contents\n"
    );
    fs::write(&patch, "keep existing output").unwrap();
    assert!(repo.export_commit_patch("--all", None, &patch).is_err());
    assert_eq!(fs::read_to_string(&patch).unwrap(), "keep existing output");
}

#[test]
fn amend_refuses_staged_contents_changed_since_the_menu_was_opened() {
    let (_directory, repo) = fixture();
    let tip = commit_file(&repo, "a", "committed", "Original");
    fs::write(repo.root().join("a"), "first staged version").unwrap();
    git(&repo, &["add", "a"]);
    let context = repo.commit_context(&tip).unwrap();
    fs::write(repo.root().join("a"), "new staged version").unwrap();
    git(&repo, &["add", "a"]);
    assert!(matches!(
        repo.prepare_commit_operation(
            &context,
            Op::Amend {
                message: "New".into(),
                include_staged: true
            }
        ),
        Err(Error::Rule(RuleError::StaleCommit))
    ));
    assert_eq!(head(&repo), tip);
    assert_eq!(git(&repo, &["show", ":a"]), "new staged version");
}

#[test]
fn newly_published_history_invalidates_a_previously_confirmed_rewrite() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "a", "base", "Base");
    let tip = commit_file(&repo, "a", "tip", "Tip");
    let context = repo.commit_context(&base).unwrap();
    let prepared = repo
        .prepare_commit_operation(&context, Op::Reset(ResetMode::Soft))
        .unwrap();
    assert!(prepared.published.is_empty());
    git(&repo, &["update-ref", "refs/remotes/origin/main", &tip]);
    assert!(matches!(
        repo.apply_commit_operation(&prepared, &editor()),
        Err(Error::Rule(RuleError::StaleCommit))
    ));
    assert_eq!(head(&repo), tip);
}
