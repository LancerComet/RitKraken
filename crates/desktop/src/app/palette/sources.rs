use std::collections::BTreeMap;

use ritkraken_git::{self as git, Change, FileTarget, RefKind, RefRequest, Scope, Snapshot};

use super::model::{Candidate, Command, CommandContext, Target};

pub(super) fn immediate(context: Option<CommandContext<'_>>) -> Vec<Candidate> {
    let mut candidates: Vec<_> = Command::ALL
        .into_iter()
        .filter(|command| command.available(context))
        .map(|command| Candidate {
            title: command.label(),
            details: String::new(),
            target: Target::Command(command),
        })
        .collect();
    if let Some(context) = context.filter(|c| !c.busy && !c.rebase && c.snapshot.sequence.is_none())
    {
        let snapshot = context.snapshot;
        candidates.extend(
            snapshot
                .refs
                .iter()
                .filter(|reference| match reference.kind {
                    RefKind::Local => {
                        !reference.checked_out && !snapshot.is_current_branch(reference)
                    }
                    RefKind::Remote => git::remote_branch(&reference.name, &snapshot.remotes)
                        .is_some_and(|(_, name)| {
                            !snapshot
                                .refs
                                .iter()
                                .any(|local| local.kind == RefKind::Local && local.name == name)
                        }),
                    RefKind::Tag => false,
                })
                .map(|reference| Candidate {
                    title: reference.name.clone(),
                    details: reference.full_name(),
                    target: Target::Branch(RefRequest::checkout(snapshot, reference.clone())),
                }),
        );
    }
    candidates
}

pub(super) fn files(snapshot: &Snapshot, committed: Vec<Change>) -> Vec<Candidate> {
    let mut targets = BTreeMap::new();
    for change in committed {
        targets.insert(
            change.path.clone(),
            FileTarget {
                scope: Scope::Worktree,
                change,
            },
        );
    }
    for change in &snapshot.status.staged {
        if change.code == b'R'
            && let Some(previous) = &change.previous
        {
            targets.remove(previous);
        }
        // A staged deletion still has its last contents in HEAD.
        let scope = if change.code == b'D' {
            snapshot.head.as_ref().map(|id| Scope::Commit {
                id: id.clone(),
                parent: None,
            })
        } else {
            Some(Scope::Index)
        };
        if let Some(scope) = scope {
            let mut change = change.clone();
            if change.code == b'D' {
                change.code = b' ';
            }
            targets.insert(change.path.clone(), FileTarget { scope, change });
        }
    }
    for change in &snapshot.status.unstaged {
        if change.code == b'R'
            && let Some(previous) = &change.previous
        {
            targets.remove(previous);
        }
        // An unstaged deletion still has its last contents in the index.
        let (scope, change) = if change.code == b'D' {
            let mut change = change.clone();
            change.code = b' ';
            (Scope::Index, change)
        } else {
            (Scope::Worktree, change.clone())
        };
        targets.insert(change.path.clone(), FileTarget { scope, change });
    }
    targets
        .into_values()
        .map(|target| Candidate {
            title: target.change.path.to_string_lossy().into_owned(),
            details: match &target.scope {
                Scope::Worktree => String::new(),
                Scope::Index => crate::i18n::tr!("app.palette.staged_file"),
                Scope::Commit { id, .. } => git::short_id(id).to_owned(),
            },
            target: Target::File(target),
        })
        .collect()
}

pub(super) fn commits(commits: Vec<git::Commit>) -> Vec<Candidate> {
    commits
        .into_iter()
        .map(|commit| Candidate {
            title: commit.subject,
            details: format!("{}  {}  {}", commit.id, commit.author, commit.date),
            target: Target::Commit(commit.id),
        })
        .collect()
}
