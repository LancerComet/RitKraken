// Included by the worktrees integration test to share its real Git fixture.
use super::{
    Error, NewWorktree, Repo, RuleError, add, command, commit_file, fixture, fs, git_output,
};

#[test]
fn lists_detached_lock_reasons_and_missing_prunable_entries() {
    let (_directory, repo) = fixture();
    let id = commit_file(&repo, "tracked", "base", "Base");
    let targets = tempfile::tempdir().unwrap();
    let detached = targets.path().join("detached space 路径");
    git_output(
        command(Some(repo.root()))
            .args(["worktree", "add", "--detach", "--"])
            .arg(&detached)
            .arg("HEAD"),
    );
    let missing = add(&repo, &targets.path().join("missing"), "missing");
    git_output(
        command(Some(repo.root()))
            .args(["worktree", "lock", "--reason", "reason\n路径", "--"])
            .arg(&detached),
    );
    fs::remove_dir_all(&missing.path).unwrap();

    let entries = repo.worktrees().unwrap();
    let entry = entries.iter().find(|entry| entry.branch.is_none()).unwrap();
    assert_eq!(
        entry.path.canonicalize().unwrap(),
        detached.canonicalize().unwrap()
    );
    assert_eq!(entry.head.as_deref(), Some(id.as_str()));
    assert_eq!(entry.locked.as_deref(), Some("reason\n路径"));
    let missing = entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("missing"))
        .unwrap();
    assert!(
        missing
            .prunable
            .as_ref()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert!(!missing.dirty);
    assert!(matches!(
        repo.remove_worktree(missing),
        Err(Error::Rule(RuleError::StaleWorktree))
    ));
    assert!(
        repo.worktrees()
            .unwrap()
            .iter()
            .any(|entry| entry.branch.as_deref() == Some("missing"))
    );
}

#[test]
fn linked_worktree_of_bare_repository_reports_protected_bare_main() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "tracked", "base", "Base");
    let targets = tempfile::tempdir().unwrap();
    let bare = targets.path().join("bare.git");
    git_output(
        command(None)
            .args(["clone", "--bare", "--"])
            .arg(repo.root())
            .arg(&bare),
    );
    let linked = targets.path().join("linked");
    git_output(
        command(Some(&bare))
            .args(["worktree", "add", "--"])
            .arg(&linked)
            .arg("main"),
    );
    let linked = Repo::open(&linked).unwrap();

    let entries = linked.worktrees().unwrap();

    let main = entries.iter().find(|entry| entry.is_main).unwrap();
    assert!(main.bare);
    assert!(!main.dirty);
    assert!(!main.is_current);
    assert!(matches!(
        linked.remove_worktree(main),
        Err(Error::Rule(RuleError::ProtectedWorktree))
    ));
}

#[test]
fn empty_repository_lists_unborn_main_and_refuses_creation() {
    let (_directory, repo) = fixture();
    let targets = tempfile::tempdir().unwrap();
    let entries = repo.worktrees().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].head, None);
    assert_eq!(entries[0].branch.as_deref(), Some("main"));
    assert!(matches!(
        repo.add_worktree(&NewWorktree {
            path: targets.path().join("linked"),
            branch: "topic".into(),
            create_branch: true,
        }),
        Err(Error::Rule(RuleError::InvalidCommit))
    ));
}
