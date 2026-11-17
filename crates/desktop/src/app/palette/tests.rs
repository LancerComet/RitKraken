use ritkraken_git::{Change, RefKind, Reference, Scope, Snapshot, Upstream};

use super::{
    model::{Command, CommandContext, Target},
    sources,
};

pub(super) fn snapshot() -> Snapshot {
    Snapshot {
        branch: Some("main".into()),
        head: Some("head".into()),
        status: Default::default(),
        refs: vec![reference("main", true)],
        remotes: vec!["origin".into()],
        upstream: Some(Upstream {
            remote: "origin".into(),
            reference: "refs/heads/main".into(),
        }),
        has_stash: false,
        sequence: None,
        menu_metadata: Default::default(),
        commits: Vec::new(),
        history_limited: false,
    }
}

pub(super) fn reference(name: &str, checked_out: bool) -> Reference {
    Reference {
        name: name.into(),
        kind: RefKind::Local,
        id: "tip".into(),
        object_id: "tip".into(),
        checked_out,
    }
}

pub(super) fn change(path: &str, code: u8) -> Change {
    Change {
        path: path.into(),
        previous: None,
        code,
    }
}

#[test]
fn no_repository_exposes_only_global_commands() {
    let commands: Vec<_> = Command::ALL
        .into_iter()
        .filter(|command| command.available(None))
        .collect();
    assert_eq!(commands, [Command::OpenRepos, Command::Preferences]);
}

#[test]
fn contextual_commands_follow_repository_capabilities() {
    let mut snapshot = snapshot();
    fn context(snapshot: &Snapshot) -> Option<CommandContext<'_>> {
        Some(CommandContext {
            snapshot,
            busy: false,
            rebase: false,
        })
    }
    assert!(Command::Push.available(context(&snapshot)));
    assert!(!Command::StageAll.available(context(&snapshot)));
    assert!(!Command::UnstageAll.available(context(&snapshot)));
    assert!(!Command::Stash.available(context(&snapshot)));
    assert!(!Command::Pop.available(context(&snapshot)));
    snapshot.status.unstaged.push(change("new.txt", b'?'));
    snapshot.status.staged.push(change("staged.txt", b'A'));
    snapshot.has_stash = true;
    for command in [
        Command::StageAll,
        Command::UnstageAll,
        Command::Stash,
        Command::Pop,
    ] {
        assert!(command.available(context(&snapshot)), "{command:?}");
    }
    snapshot.status.unstaged.push(change("conflict.txt", b'U'));
    assert!(!Command::Stash.available(context(&snapshot)));
    assert!(!Command::StageAll.available(context(&snapshot)));
    snapshot.upstream = None;
    assert!(!Command::Push.available(context(&snapshot)));
    snapshot.head = None;
    assert!(!Command::CreateBranch.available(context(&snapshot)));
    assert!(!Command::Stash.available(context(&snapshot)));
    snapshot.remotes.clear();
    assert!(!Command::Fetch.available(context(&snapshot)));
}

#[test]
fn busy_or_rebase_context_excludes_mutating_and_modal_repository_commands() {
    let snapshot = snapshot();
    for (busy, rebase) in [(true, false), (false, true)] {
        let context = Some(CommandContext {
            snapshot: &snapshot,
            busy,
            rebase,
        });
        let commands: Vec<_> = Command::ALL
            .into_iter()
            .filter(|command| command.available(context))
            .collect();
        let mut expected = vec![Command::OpenRepos, Command::Preferences];
        if !busy {
            expected.push(Command::Refresh);
        }
        assert_eq!(commands, expected);
    }
}

#[test]
fn branch_candidates_exclude_current_and_other_worktrees_and_capture_stale_check() {
    let mut snapshot = snapshot();
    snapshot.refs[0].checked_out = false;
    snapshot
        .refs
        .extend([reference("occupied", true), reference("feature", false)]);
    let candidates = sources::immediate(Some(CommandContext {
        snapshot: &snapshot,
        busy: false,
        rebase: false,
    }));
    let branches: Vec<_> = candidates
        .iter()
        .filter_map(|candidate| match &candidate.target {
            Target::Branch(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].reference.name, "feature");
    assert_eq!(branches[0].head.as_deref(), Some("head"));
    assert_eq!(branches[0].branch.as_deref(), Some("main"));
}

#[test]
fn file_candidates_deduplicate_and_keep_readable_versions_for_deletions() {
    let mut snapshot = snapshot();
    snapshot.status.staged = vec![change("staged.txt", b'M'), change("removed.txt", b'D')];
    snapshot.status.unstaged = vec![
        change("staged.txt", b'M'),
        change("missing.txt", b'D'),
        change("new.txt", b'?'),
    ];
    let candidates = sources::files(
        &snapshot,
        vec![
            change("staged.txt", b' '),
            change("removed.txt", b' '),
            change("missing.txt", b' '),
            change("clean.txt", b' '),
        ],
    );
    let files: Vec<_> = candidates
        .into_iter()
        .filter_map(|candidate| match candidate.target {
            Target::File(file) => Some(file),
            _ => None,
        })
        .collect();
    let actual: Vec<_> = files
        .iter()
        .map(|file| {
            (
                file.change.path.to_str().unwrap(),
                file.scope.clone(),
                file.change.code,
            )
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            ("clean.txt", Scope::Worktree, b' '),
            ("missing.txt", Scope::Index, b' '),
            ("new.txt", Scope::Worktree, b'?'),
            (
                "removed.txt",
                Scope::Commit {
                    id: "head".into(),
                    parent: None
                },
                b' '
            ),
            ("staged.txt", Scope::Worktree, b'M'),
        ]
    );
}

#[test]
fn unborn_repository_files_include_index_and_untracked_paths() {
    let mut snapshot = snapshot();
    snapshot.head = None;
    snapshot.status.staged = vec![change("staged.txt", b'A')];
    snapshot.status.unstaged = vec![change("new.txt", b'?')];
    let candidates = sources::files(&snapshot, Vec::new());
    let actual: Vec<_> = candidates
        .into_iter()
        .filter_map(|candidate| match candidate.target {
            Target::File(file) => Some((file.change.path, file.scope)),
            _ => None,
        })
        .collect();
    assert_eq!(
        actual,
        [
            ("new.txt".into(), Scope::Worktree),
            ("staged.txt".into(), Scope::Index),
        ]
    );
}

#[test]
fn renamed_files_replace_the_old_committed_path() {
    let mut snapshot = snapshot();
    snapshot.status.staged = vec![Change {
        path: "renamed.txt".into(),
        previous: Some("old.txt".into()),
        code: b'R',
    }];
    let candidates = sources::files(&snapshot, vec![change("old.txt", b' ')]);
    let actual: Vec<_> = candidates
        .into_iter()
        .filter_map(|candidate| match candidate.target {
            Target::File(file) => Some((file.change.path, file.scope)),
            _ => None,
        })
        .collect();
    assert_eq!(actual, [("renamed.txt".into(), Scope::Index)]);
}
