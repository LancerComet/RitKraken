mod support;

use std::fs;

use ritkraken_git::{CommitOperation as Op, SequenceKind};
use support::{commits::*, fixture};

#[test]
fn cherry_pick_stages_changes_until_the_user_makes_a_commit() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "base", "base\n", "Base");
    git(&repo, &["switch", "-c", "feature"]);
    let target = commit_file(&repo, "feature", "picked\n", "Feature");
    git(&repo, &["switch", "main"]);
    let count = git(&repo, &["rev-list", "--count", "HEAD"]);

    apply(&repo, &target, Op::CherryPick { mainline: None });

    assert_eq!(head(&repo), base);
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]), count);
    assert_eq!(git(&repo, &["show", ":feature"]), "picked\n");
    assert!(git(&repo, &["diff", "--name-only"]).trim().is_empty());
    assert_eq!(
        git(&repo, &["diff", "--cached", "--name-only"]).trim(),
        "feature"
    );
    assert_eq!(repo.sequence_kind().unwrap(), None);

    repo.commit("Reviewed changes", "My commit message")
        .unwrap();
    assert_ne!(head(&repo), base);
    assert_eq!(
        repo.commit_context(&head(&repo)).unwrap().message,
        "Reviewed changes\n\nMy commit message"
    );
    assert!(repo.status().unwrap().is_empty());
}

#[test]
fn merge_cherry_pick_applies_only_the_selected_parent_diff_without_committing() {
    for mainline in [1, 2] {
        let (_directory, repo) = fixture();
        let base = commit_file(&repo, "base", "base\n", "Base");
        git(&repo, &["switch", "-c", "feature"]);
        commit_file(&repo, "feature", "feature\n", "Feature");
        git(&repo, &["switch", "main"]);
        commit_file(&repo, "main", "main\n", "Main");
        git(&repo, &["merge", "--no-edit", "feature"]);
        let target = head(&repo);
        git(&repo, &["switch", "-c", "receiver", &base]);

        apply(
            &repo,
            &target,
            Op::CherryPick {
                mainline: Some(mainline),
            },
        );

        assert_eq!(head(&repo), base);
        assert_eq!(
            git(&repo, &["diff", "--cached", "--name-only"]).trim(),
            if mainline == 1 { "feature" } else { "main" }
        );
        assert_eq!(repo.sequence_kind().unwrap(), None);
    }
}

#[test]
fn root_commit_can_be_cherry_picked_without_committing() {
    let (_directory, repo) = fixture();
    let main = commit_file(&repo, "main", "main\n", "Main");
    git(&repo, &["switch", "--orphan", "feature"]);
    let target = commit_file(&repo, "feature", "root\n", "Root");
    git(&repo, &["switch", "main"]);

    apply(&repo, &target, Op::CherryPick { mainline: None });

    assert_eq!(head(&repo), main);
    assert_eq!(git(&repo, &["show", ":feature"]), "root\n");
    assert_eq!(repo.sequence_kind().unwrap(), None);
}

#[test]
fn conflicted_picks_continue_without_a_commit_and_support_skip_and_abort() {
    for root in [false, true] {
        for action in [Op::Continue, Op::Skip, Op::Abort] {
            let (_directory, repo) = fixture();
            commit_file(&repo, "a", "base\n", "Base");
            git(
                &repo,
                if root {
                    &["switch", "--orphan", "feature"]
                } else {
                    &["switch", "-c", "feature"]
                },
            );
            let target = commit_file(&repo, "a", "feature\n", "Feature");
            git(&repo, &["switch", "main"]);
            let main = commit_file(&repo, "a", "main\n", "Main");
            let context = repo.commit_context(&target).unwrap();
            let prepared = repo
                .prepare_commit_operation(&context, Op::CherryPick { mainline: None })
                .unwrap();
            assert!(repo.apply_commit_operation(&prepared, &editor()).is_err());
            assert_eq!(
                repo.sequence_kind().unwrap(),
                Some(SequenceKind::CherryPick)
            );

            let continuing = matches!(action, Op::Continue);
            if continuing {
                fs::write(repo.root().join("a"), "resolved\n").unwrap();
                git(&repo, &["add", "a"]);
            }
            apply(&repo, &main, action);

            assert_eq!(head(&repo), main);
            assert_eq!(repo.sequence_kind().unwrap(), None);
            assert_eq!(
                git(&repo, &["show", ":a"]),
                if continuing { "resolved\n" } else { "main\n" }
            );
            if !continuing {
                assert!(repo.status().unwrap().is_empty());
            }
        }
    }
}
