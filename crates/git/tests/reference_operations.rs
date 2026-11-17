mod support;

use ritkraken_git::{Error, RefKind, RefOperation, RefRequest, Repo, RuleError};
use support::commits::{commit_file, git, head};
use support::{command, fixture};

fn request(repo: &Repo, kind: RefKind, name: &str, operation: RefOperation) -> RefRequest {
    let snapshot = repo.snapshot(None).unwrap();
    let reference = snapshot
        .refs
        .iter()
        .find(|reference| reference.kind == kind && reference.name == name)
        .unwrap()
        .clone();
    RefRequest::new(&snapshot, reference, operation)
}

#[test]
fn branch_rename_preserves_checkout_upstream_and_nested_names() {
    let (_directory, repo) = fixture();
    let id = commit_file(&repo, "one", "one", "One");
    git(&repo, &["remote", "add", "origin", "."]);
    git(&repo, &["update-ref", "refs/remotes/origin/main", &id]);
    git(&repo, &["branch", "--set-upstream-to=origin/main", "main"]);
    let rename = request(
        &repo,
        RefKind::Local,
        "main",
        RefOperation::Rename("topic/new".into()),
    );
    repo.apply_ref_operation(&rename).unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot.branch.as_deref(), Some("topic/new"));
    assert_eq!(snapshot.upstream.unwrap().reference, "refs/heads/main");
    assert_eq!(
        git(&repo, &["config", "branch.topic/new.remote"]).trim(),
        "origin"
    );
    assert_eq!(head(&repo), id);
}

#[test]
fn annotated_tag_rename_keeps_the_tag_object_and_is_atomic_on_collision() {
    let (_directory, repo) = fixture();
    let id = commit_file(&repo, "one", "one", "One");
    git(&repo, &["tag", "-a", "release/one", "-m", "Release notes"]);
    let raw = git(&repo, &["rev-parse", "refs/tags/release/one"])
        .trim()
        .to_owned();
    assert_ne!(raw, id);
    let rename = request(
        &repo,
        RefKind::Tag,
        "release/one",
        RefOperation::Rename("release/two".into()),
    );
    assert_eq!(rename.reference.object_id, raw);
    assert_eq!(rename.reference.id, id);
    repo.apply_ref_operation(&rename).unwrap();
    assert_eq!(
        git(&repo, &["rev-parse", "refs/tags/release/two"]).trim(),
        raw
    );
    git(&repo, &["tag", "occupied"]);
    let collision = request(
        &repo,
        RefKind::Tag,
        "release/two",
        RefOperation::Rename("occupied".into()),
    );
    assert!(matches!(
        repo.apply_ref_operation(&collision),
        Err(Error::Rule(RuleError::RefExists))
    ));
    assert_eq!(
        git(&repo, &["rev-parse", "refs/tags/release/two"]).trim(),
        raw
    );
    let delete = request(&repo, RefKind::Tag, "release/two", RefOperation::Delete);
    repo.apply_ref_operation(&delete).unwrap();
    assert!(
        repo.snapshot(None)
            .unwrap()
            .refs
            .iter()
            .any(|reference| reference.name == "occupied")
    );
    assert!(
        repo.snapshot(None)
            .unwrap()
            .refs
            .iter()
            .all(|reference| reference.name != "release/two")
    );
}

#[test]
fn stale_reference_and_changed_checkout_are_rejected() {
    let (_directory, repo) = fixture();
    let first = commit_file(&repo, "one", "one", "One");
    git(&repo, &["branch", "topic"]);
    let rename = request(
        &repo,
        RefKind::Local,
        "topic",
        RefOperation::Rename("new".into()),
    );
    let second = commit_file(&repo, "two", "two", "Two");
    assert!(matches!(
        repo.apply_ref_operation(&rename),
        Err(Error::Rule(RuleError::StaleReference))
    ));
    let rename = request(
        &repo,
        RefKind::Local,
        "topic",
        RefOperation::Rename("new".into()),
    );
    git(&repo, &["update-ref", "refs/heads/topic", &second, &first]);
    assert!(matches!(
        repo.apply_ref_operation(&rename),
        Err(Error::Rule(RuleError::StaleReference))
    ));
    assert_eq!(git(&repo, &["rev-parse", "topic"]).trim(), second);
}

#[test]
fn branch_deletion_blocks_every_checked_out_worktree() {
    let (directory, repo) = fixture();
    commit_file(&repo, "one", "one", "One");
    let current = request(&repo, RefKind::Local, "main", RefOperation::Delete);
    assert!(matches!(
        repo.apply_ref_operation(&current),
        Err(Error::Rule(RuleError::BranchInUse))
    ));
    git(&repo, &["branch", "linked"]);
    let linked = directory.path().join("linked worktree");
    let output = command(Some(repo.root()))
        .args(["worktree", "add"])
        .arg(&linked)
        .arg("linked")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for operation in [
        RefOperation::Delete,
        RefOperation::Rename("renamed".into()),
        RefOperation::Checkout,
    ] {
        let request = request(&repo, RefKind::Local, "linked", operation);
        assert!(request.reference.checked_out);
        assert!(matches!(
            repo.apply_ref_operation(&request),
            Err(Error::Rule(RuleError::BranchInUse))
        ));
    }
    git(&repo, &["branch", "merged"]);
    repo.apply_ref_operation(&request(
        &repo,
        RefKind::Local,
        "merged",
        RefOperation::Delete,
    ))
    .unwrap();
    assert!(
        repo.snapshot(None)
            .unwrap()
            .refs
            .iter()
            .all(|reference| reference.name != "merged")
    );
}

#[test]
fn confirmed_branch_deletion_is_independent_of_its_relation_to_head() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "one", "one", "One");
    git(&repo, &["switch", "-c", "abc"]);
    let tip = commit_file(&repo, "two", "two", "Two");
    git(&repo, &["switch", "main"]);
    // Match the reported repository: abc is a direct child of the current branch.
    std::fs::write(repo.root().join("one"), "local edits").unwrap();
    std::fs::write(repo.root().join("staged"), "staged edits").unwrap();
    git(&repo, &["add", "staged"]);
    let status = repo.status().unwrap();
    repo.apply_ref_operation(&request(&repo, RefKind::Local, "abc", RefOperation::Delete))
        .unwrap();
    assert_eq!(head(&repo), base);
    assert_eq!(repo.status().unwrap(), status);
    assert_eq!(
        std::fs::read_to_string(repo.root().join("one")).unwrap(),
        "local edits"
    );
    assert!(!repo.root().join("two").exists());
    assert!(
        repo.snapshot(None)
            .unwrap()
            .refs
            .iter()
            .all(|reference| reference.name != "abc")
    );
    // Deleting a branch removes its reference, not the commit object.
    assert_eq!(git(&repo, &["rev-parse", &tip]).trim(), tip);
}

#[test]
fn branch_deletion_rechecks_the_reference_before_removing_it() {
    let (_directory, repo) = fixture();
    let first = commit_file(&repo, "one", "one", "One");
    git(&repo, &["branch", "abc"]);
    let second = commit_file(&repo, "two", "two", "Two");
    let delete = request(&repo, RefKind::Local, "abc", RefOperation::Delete);
    git(&repo, &["update-ref", "refs/heads/abc", &second, &first]);
    assert!(matches!(
        repo.apply_ref_operation(&delete),
        Err(Error::Rule(RuleError::StaleReference))
    ));
    assert_eq!(git(&repo, &["rev-parse", "abc"]).trim(), second);
    assert_eq!(head(&repo), second);
}

#[test]
fn reference_checkout_and_tracking_branch_use_the_selected_reference() {
    let (_directory, repo) = fixture();
    let first = commit_file(&repo, "one", "one", "One");
    git(&repo, &["branch", "topic"]);
    git(&repo, &["tag", "v1"]);
    git(&repo, &["remote", "add", "origin", "."]);
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/nested/topic", &first],
    );
    commit_file(&repo, "two", "two", "Two");
    repo.apply_ref_operation(&request(&repo, RefKind::Tag, "v1", RefOperation::Checkout))
        .unwrap();
    assert_eq!(repo.snapshot(None).unwrap().branch, None);
    assert_eq!(head(&repo), first);
    repo.apply_ref_operation(&request(
        &repo,
        RefKind::Local,
        "topic",
        RefOperation::Checkout,
    ))
    .unwrap();
    assert_eq!(
        repo.snapshot(None).unwrap().branch.as_deref(),
        Some("topic")
    );
    let snapshot = repo.snapshot(None).unwrap();
    let remote = snapshot
        .refs
        .iter()
        .find(|reference| {
            reference.kind == RefKind::Remote && reference.name == "origin/nested/topic"
        })
        .unwrap()
        .clone();
    let mut checkout = RefRequest::checkout(&snapshot, remote);
    let RefOperation::Track(name) = &mut checkout.operation else {
        panic!("remote checkout must request a local tracking branch")
    };
    assert_eq!(name, "nested/topic");
    *name = "local/topic".into();
    repo.apply_ref_operation(&checkout).unwrap();
    let snapshot = repo.snapshot(None).unwrap();
    assert_eq!(snapshot.branch.as_deref(), Some("local/topic"));
    assert_eq!(
        snapshot.upstream.unwrap().reference,
        "refs/heads/nested/topic"
    );
}

#[test]
fn remote_delete_lease_rejects_moved_server_branch_and_leaves_local_branch_alone() {
    let (directory, repo) = fixture();
    let first = commit_file(&repo, "one", "one", "One");
    let remote = directory.path().join("remote.git");
    assert!(
        command(None)
            .args(["init", "--bare"])
            .arg(&remote)
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = command(Some(repo.root()))
        .args(["remote", "add", "origin"])
        .arg(&remote)
        .output()
        .unwrap();
    assert!(output.status.success());
    git(&repo, &["push", "origin", "main:refs/heads/topic"]);
    git(&repo, &["branch", "topic"]);
    let delete = request(&repo, RefKind::Remote, "origin/topic", RefOperation::Delete);
    let second = commit_file(&repo, "two", "two", "Two");
    // Move only the server branch, then recapture HEAD while retaining the old tracking ref.
    git(&repo, &["push", "origin", "main:refs/heads/unrelated"]);
    let output = command(Some(&remote))
        .args(["update-ref", "refs/heads/topic", &second, &first])
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut stale_remote = delete;
    stale_remote.head = Some(second.clone());
    assert!(repo.apply_ref_operation(&stale_remote).is_err());
    git(&repo, &["fetch", "origin"]);
    let delete = request(&repo, RefKind::Remote, "origin/topic", RefOperation::Delete);
    repo.apply_ref_operation(&delete).unwrap();
    assert_eq!(git(&repo, &["rev-parse", "topic"]).trim(), first);
    assert_eq!(head(&repo), second);
    let output = command(Some(&remote))
        .args(["show-ref", "--verify", "refs/heads/topic"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn invalid_names_cannot_be_interpreted_as_options_or_overwrite_references() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "one", "one", "One");
    for name in ["--force", "HEAD", "bad name", "bad\nname", "../main"] {
        let rename = request(
            &repo,
            RefKind::Local,
            "main",
            RefOperation::Rename(name.into()),
        );
        assert!(matches!(
            repo.apply_ref_operation(&rename),
            Err(Error::Rule(RuleError::InvalidRefName))
        ));
    }
    assert_eq!(repo.snapshot(None).unwrap().branch.as_deref(), Some("main"));
}

fn remote_tag(remote: &std::path::Path, name: &str) -> Option<String> {
    let output = command(Some(remote))
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{name}"),
        ])
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).unwrap().trim().to_owned())
}

#[test]
fn tags_push_their_reviewed_object_without_overwriting_a_different_remote_tag() {
    let (directory, repo) = fixture();
    let remote = directory.path().join("remote.git");
    support::git_output(command(None).args(["init", "--bare"]).arg(&remote));
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let first = commit_file(&repo, "one", "one", "One");
    git(&repo, &["tag", "light"]);
    git(&repo, &["tag", "-a", "release/one", "-m", "Release notes"]);
    let annotated = git(&repo, &["rev-parse", "refs/tags/release/one"])
        .trim()
        .to_owned();
    for name in ["light", "release/one"] {
        let push = request(
            &repo,
            RefKind::Tag,
            name,
            RefOperation::PushTag("origin".into()),
        );
        repo.apply_ref_operation(&push).unwrap();
    }
    assert_eq!(
        remote_tag(&remote, "light").as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        remote_tag(&remote, "release/one").as_deref(),
        Some(annotated.as_str())
    );

    commit_file(&repo, "two", "two", "Two");
    git(&repo, &["tag", "-f", "light"]);
    let moved = request(
        &repo,
        RefKind::Tag,
        "light",
        RefOperation::PushTag("origin".into()),
    );
    assert!(matches!(
        repo.apply_ref_operation(&moved),
        Err(Error::Git { .. })
    ));
    assert_eq!(
        remote_tag(&remote, "light").as_deref(),
        Some(first.as_str())
    );

    git(&repo, &["tag", "late"]);
    let stale = request(
        &repo,
        RefKind::Tag,
        "late",
        RefOperation::PushTag("origin".into()),
    );
    git(&repo, &["tag", "-f", "late", &first]);
    assert!(matches!(
        repo.apply_ref_operation(&stale),
        Err(Error::Rule(RuleError::StaleReference))
    ));
    let missing = request(
        &repo,
        RefKind::Tag,
        "late",
        RefOperation::PushTag("upstream".into()),
    );
    assert!(matches!(
        repo.apply_ref_operation(&missing),
        Err(Error::Rule(RuleError::MissingRemote(_)))
    ));
    assert_eq!(remote_tag(&remote, "late"), None);
}
