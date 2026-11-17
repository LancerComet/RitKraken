#[path = "../src/worktrees/list_tests.rs"]
mod list_tests;
mod support;

use std::{fs, path::Path};

use ritkraken_git::{Error, NewWorktree, Repo, RuleError, Worktree};
use support::commits::{commit_file, git, head};
use support::{command, fixture, git_output};

fn add(repo: &Repo, path: &Path, branch: &str) -> Worktree {
    repo.add_worktree(&NewWorktree {
        path: path.to_owned(),
        branch: branch.to_owned(),
        create_branch: true,
    })
    .unwrap();
    repo.worktrees()
        .unwrap()
        .into_iter()
        .find(|entry| entry.branch.as_deref() == Some(branch))
        .unwrap()
}

#[test]
fn creates_new_from_current_head_and_checks_out_exact_existing_local_branch() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "tracked", "base\n", "Base");
    git(&repo, &["branch", "existing"]);
    let tip = commit_file(&repo, "tracked", "next\n", "Next");
    git(&repo, &["tag", "existing"]);
    let targets = tempfile::tempdir().unwrap();
    let new = add(&repo, &targets.path().join("new space 路径"), "new/topic");
    assert_eq!(new.head.as_deref(), Some(tip.as_str()));
    assert!(!new.dirty);
    assert!(!new.is_main);
    assert!(!new.is_current);
    let existing = targets.path().join("existing");
    repo.add_worktree(&NewWorktree {
        path: existing.clone(),
        branch: "existing".into(),
        create_branch: false,
    })
    .unwrap();
    let linked = Repo::open(&existing).unwrap();
    assert_eq!(head(&linked), base);
    assert_eq!(
        git(&linked, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/existing"
    );
    let listed = linked.worktrees().unwrap();
    assert!(
        !listed
            .iter()
            .find(|entry| entry.is_main)
            .unwrap()
            .is_current
    );
    assert_eq!(listed.iter().filter(|entry| entry.is_current).count(), 1);
    assert_eq!(
        listed
            .iter()
            .find(|entry| entry.is_current)
            .unwrap()
            .branch
            .as_deref(),
        Some("existing")
    );
}

#[test]
fn rejects_invalid_or_unavailable_branches_without_creating_directories() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "tracked", "base", "Base");
    let targets = tempfile::tempdir().unwrap();
    for (branch, create_branch) in [
        ("", true),
        ("--detach", true),
        ("bad..name", true),
        ("@{-1}", true),
        ("HEAD", true),
        ("missing", false),
        ("main", false),
        ("main", true),
    ] {
        let path = targets.path().join("unused");
        assert!(
            repo.add_worktree(&NewWorktree {
                path: path.clone(),
                branch: branch.into(),
                create_branch,
            })
            .is_err(),
            "{branch}"
        );
        assert!(!path.exists(), "{branch}");
    }
    assert!(matches!(
        repo.add_worktree(&NewWorktree {
            path: "".into(),
            branch: "valid".into(),
            create_branch: true,
        }),
        Err(Error::Rule(RuleError::InvalidWorktreePath))
    ));
}

#[test]
fn option_shaped_relative_path_is_created_literally() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "tracked", "base", "Base");
    let entry = add(&repo, Path::new("-literal worktree"), "literal");
    assert_eq!(
        entry.path.canonicalize().unwrap(),
        repo.root()
            .join("-literal worktree")
            .canonicalize()
            .unwrap()
    );
    assert!(repo.root().join("-literal worktree/tracked").is_file());
}

#[test]
fn staged_unstaged_and_untracked_changes_prevent_removal_and_preserve_bytes() {
    for state in ["staged", "unstaged", "untracked"] {
        let (_directory, repo) = fixture();
        commit_file(&repo, "tracked", "base", "Base");
        let targets = tempfile::tempdir().unwrap();
        let entry = add(&repo, &targets.path().join("linked"), "topic");
        let path = entry.path.join(if state == "untracked" {
            "new"
        } else {
            "tracked"
        });
        fs::write(&path, b"valuable\0bytes").unwrap();
        if state == "staged" {
            git_output(command(Some(&entry.path)).args(["add", "tracked"]));
        }
        assert!(
            repo.worktrees()
                .unwrap()
                .iter()
                .find(|entry| entry.branch.as_deref() == Some("topic"))
                .unwrap()
                .dirty
        );

        assert!(
            matches!(
                repo.remove_worktree(&entry),
                Err(Error::Rule(RuleError::DirtyWorktree))
            ),
            "{state}"
        );
        assert_eq!(fs::read(path).unwrap(), b"valuable\0bytes");
        if state == "staged" {
            assert_eq!(
                git_output(command(Some(&entry.path)).args(["show", ":tracked"])),
                b"valuable\0bytes"
            );
        }
    }
}

#[test]
fn refuses_main_current_and_newly_locked_worktrees() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "tracked", "base", "Base");
    let main = repo.worktrees().unwrap().remove(0);
    assert!(main.is_main && main.is_current);
    assert!(matches!(
        repo.remove_worktree(&main),
        Err(Error::Rule(RuleError::ProtectedWorktree))
    ));
    let targets = tempfile::tempdir().unwrap();
    let entry = add(&repo, &targets.path().join("linked"), "topic");
    let linked = Repo::open(&entry.path).unwrap();
    assert!(matches!(
        linked.remove_worktree(&entry),
        Err(Error::Rule(RuleError::ProtectedWorktree))
    ));
    git_output(
        command(Some(repo.root()))
            .args(["worktree", "lock", "--"])
            .arg(&entry.path),
    );
    assert!(matches!(
        repo.remove_worktree(&entry),
        Err(Error::Rule(RuleError::LockedWorktree))
    ));
    assert!(entry.path.join("tracked").is_file());
}

#[test]
fn refuses_changed_branch_head_and_moved_path_without_deleting_another_entry() {
    for change in ["branch", "head", "path"] {
        let (_directory, repo) = fixture();
        commit_file(&repo, "tracked", "base", "Base");
        let targets = tempfile::tempdir().unwrap();
        let entry = add(&repo, &targets.path().join("linked"), "topic");
        let other = add(&repo, &targets.path().join("other"), "other");
        match change {
            "branch" => {
                git_output(command(Some(&entry.path)).args(["switch", "-c", "replacement"]));
            }
            "head" => {
                commit_file(&Repo::open(&entry.path).unwrap(), "tracked", "next", "Next");
            }
            "path" => {
                git_output(
                    command(Some(repo.root()))
                        .args(["worktree", "move", "--"])
                        .arg(&entry.path)
                        .arg(targets.path().join("moved")),
                );
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                repo.remove_worktree(&entry),
                Err(Error::Rule(RuleError::StaleWorktree))
            ),
            "{change}"
        );
        assert!(other.path.join("tracked").is_file());
        assert_eq!(repo.worktrees().unwrap().len(), 3);
    }
}

#[test]
fn removing_clean_worktree_retains_its_branch() {
    let (_directory, repo) = fixture();
    let id = commit_file(&repo, "tracked", "base", "Base");
    let targets = tempfile::tempdir().unwrap();
    let entry = add(&repo, &targets.path().join("linked"), "topic");

    repo.remove_worktree(&entry).unwrap();

    assert!(!entry.path.exists());
    assert_eq!(repo.worktrees().unwrap().len(), 1);
    assert_eq!(git(&repo, &["rev-parse", "refs/heads/topic"]).trim(), id);
}
