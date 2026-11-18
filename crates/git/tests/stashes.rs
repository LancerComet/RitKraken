mod support;

use std::fs;

use ritkraken_git::{Error, Repo, RuleError};
use support::commits::{commit_file, git};
use support::fixture;

fn stash_with(repo: &Repo, file: &str, contents: &str) {
    fs::write(repo.root().join(file), contents).unwrap();
    repo.stash().unwrap();
}

#[test]
fn stashes_appear_in_history_above_their_base_without_helper_commits() {
    let (_directory, repo) = fixture();
    let base = commit_file(&repo, "tracked", "base\n", "Base");
    stash_with(&repo, "tracked", "first\n");
    stash_with(&repo, "untracked", "new file\n");

    let snapshot = repo.snapshot(None).unwrap();
    assert!(snapshot.has_stash);
    let indexes: Vec<_> = snapshot.stashes.iter().map(|stash| stash.index).collect();
    assert_eq!(indexes, [0, 1]);
    assert!(snapshot.stashes.iter().all(|stash| stash.base == base));
    assert!(snapshot.stashes[0].message.starts_with("WIP on main"));

    let ids: Vec<_> = snapshot
        .commits
        .iter()
        .map(|commit| commit.id.as_str())
        .collect();
    let base_row = ids.iter().position(|id| *id == base).unwrap();
    for stash in &snapshot.stashes {
        let row = ids
            .iter()
            .position(|id| *id == stash.id)
            .expect("stash row");
        assert!(row < base_row, "stash listed above its base");
        assert_eq!(snapshot.commits[row].parents, std::slice::from_ref(&base));
    }
    assert_eq!(
        snapshot.commits.len(),
        3,
        "only base and two stashes: {ids:?}"
    );
}

#[test]
fn applying_or_dropping_acts_on_the_chosen_stash_and_rejects_a_changed_list() {
    let (_directory, repo) = fixture();
    commit_file(&repo, "tracked", "base\n", "Base");
    stash_with(&repo, "tracked", "older\n");
    stash_with(&repo, "tracked", "newer\n");
    let stashes = repo.snapshot(None).unwrap().stashes;
    let (newer, older) = (&stashes[0], &stashes[1]);

    repo.apply_stash(older, false).unwrap();
    assert_eq!(
        fs::read_to_string(repo.root().join("tracked")).unwrap(),
        "older\n"
    );
    assert_eq!(repo.snapshot(None).unwrap().stashes.len(), 2);
    git(&repo, &["checkout", "--", "tracked"]);

    repo.drop_stash(newer).unwrap();
    let remaining = repo.snapshot(None).unwrap().stashes;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, older.id);
    assert!(matches!(
        repo.apply_stash(older, false),
        Err(Error::Rule(RuleError::StaleStash))
    ));
    assert!(matches!(
        repo.drop_stash(newer),
        Err(Error::Rule(RuleError::StaleStash))
    ));

    stash_with(&repo, "tracked", "latest\n");
    repo.pop_stash().unwrap();
    assert_eq!(
        fs::read_to_string(repo.root().join("tracked")).unwrap(),
        "latest\n"
    );
    let left = repo.snapshot(None).unwrap().stashes;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, older.id);
}
