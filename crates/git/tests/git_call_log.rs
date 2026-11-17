mod support;

use std::sync::{Arc, Mutex};

use ritkraken_git::{GitCall, NewRemote, Repo, set_git_call_listener};
use support::fixture;

#[test]
fn every_git_process_is_reported_with_outcome_and_without_credentials() {
    let calls: Arc<Mutex<Vec<GitCall>>> = Arc::default();
    let sink = calls.clone();
    assert!(set_git_call_listener(move |call| sink
        .lock()
        .unwrap()
        .push(call.clone())));

    let (_directory, repo) = fixture();
    repo.snapshot(None).unwrap();
    let remote = NewRemote::new(
        "origin",
        "https://user:secret-token@example.com/a.git",
        None,
    )
    .unwrap();
    repo.add_remote(&remote).unwrap();
    let outside = tempfile::Builder::new()
        .prefix("ritkraken-not-a-repository-")
        .tempdir()
        .unwrap();
    assert!(Repo::open(outside.path()).is_err());

    let calls = calls.lock().unwrap();
    let status = calls
        .iter()
        .find(|call| call.args.first().map(String::as_str) == Some("status"))
        .expect("status call recorded");
    assert_eq!(status.directory.as_deref(), Some(repo.root()));
    assert_eq!(status.exit_code, Some(0));
    assert!(!status.failed());

    let add = calls
        .iter()
        .find(|call| call.args.starts_with(&["remote".into(), "add".into()]))
        .expect("remote add recorded");
    assert!(add.args.iter().all(|arg| !arg.contains("secret-token")));
    assert!(
        add.args
            .contains(&"https://***@example.com/a.git".to_owned())
    );

    let failed = calls
        .iter()
        .find(|call| call.failed())
        .expect("failure recorded");
    assert_ne!(failed.exit_code, Some(0));
    assert!(
        failed
            .detail
            .as_deref()
            .is_some_and(|detail| !detail.is_empty())
    );
}
