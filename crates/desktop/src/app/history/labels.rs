mod hover;
mod view;

use std::{collections::HashMap, sync::Arc};

use ritkraken_git::{RefKind, Reference, Snapshot};

pub(super) use view::ref_labels;

#[derive(Clone, Debug)]
pub(super) struct RefBadge {
    name: String,
    short_name: Option<String>,
    kind: Option<RefKind>,
    pub(super) reference: Option<Reference>,
    checked_out: bool,
    pub(super) remote_refs: Vec<Reference>,
}

impl RefBadge {
    fn display_name(&self) -> &str {
        self.short_name.as_deref().unwrap_or(&self.name)
    }

    fn icons(&self) -> impl Iterator<Item = RefKind> {
        self.kind
            .into_iter()
            .chain((!self.remote_refs.is_empty()).then_some(RefKind::Remote))
    }

    fn tooltip(&self) -> String {
        std::iter::once(&self.name)
            .chain(self.remote_refs.iter().map(|reference| &reference.name))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub(super) fn group_refs(snapshot: &Snapshot) -> HashMap<String, Arc<[RefBadge]>> {
    let mut groups: HashMap<String, Vec<RefBadge>> = HashMap::new();
    let mut local_positions = HashMap::new();
    let head = snapshot.head.as_deref();

    for kind in [RefKind::Local, RefKind::Remote, RefKind::Tag] {
        for reference in snapshot.refs_of_kind(kind) {
            let badges = groups.entry(reference.id.clone()).or_default();
            let short_name = (kind == RefKind::Remote)
                .then(|| {
                    ritkraken_git::remote_branch(&reference.name, &snapshot.remotes)
                        .map(|(_, name)| name)
                })
                .flatten();
            if kind == RefKind::Local {
                local_positions.insert(
                    (reference.id.as_str(), reference.name.as_str()),
                    badges.len(),
                );
            } else if let Some(name) = short_name
                && let Some(&position) = local_positions.get(&(reference.id.as_str(), name))
            {
                badges[position].remote_refs.push(reference.clone());
                continue;
            }
            badges.push(RefBadge {
                name: reference.name.clone(),
                short_name: short_name.map(str::to_owned),
                kind: Some(kind),
                reference: Some(reference.clone()),
                checked_out: head == Some(reference.id.as_str())
                    && snapshot.is_current_branch(reference),
                remote_refs: Vec::new(),
            });
        }
    }

    if let Some(head) = head {
        let badges = groups.entry(head.to_owned()).or_default();
        if snapshot.branch.is_none() {
            badges.insert(
                0,
                RefBadge {
                    name: "HEAD".into(),
                    short_name: None,
                    kind: None,
                    reference: None,
                    checked_out: true,
                    remote_refs: Vec::new(),
                },
            );
        } else {
            badges.sort_by_key(|badge| !badge.checked_out);
        }
    }
    groups
        .into_iter()
        .map(|(id, badges)| (id, badges.into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::group_refs;
    use ritkraken_git::{RefKind, Reference, Snapshot};

    fn reference(kind: RefKind, name: &str, id: &str) -> Reference {
        Reference {
            kind,
            name: name.into(),
            id: id.into(),
            object_id: id.into(),
            checked_out: false,
        }
    }

    fn snapshot(branch: Option<&str>, refs: Vec<Reference>) -> Snapshot {
        Snapshot {
            menu_metadata: Default::default(),
            sequence: None,
            branch: branch.map(str::to_owned),
            head: Some("head".into()),
            status: Default::default(),
            refs,
            remotes: vec!["origin".into(), "fork".into(), "fork/review".into()],
            upstream: None,
            has_stash: false,
            commits: Vec::new(),
            history_limited: false,
        }
    }

    #[test]
    fn matching_branches_share_a_badge_and_tags_keep_their_own() {
        let groups = group_refs(&snapshot(
            Some("feature/topic"),
            vec![
                reference(RefKind::Remote, "origin/feature/topic", "head"),
                reference(RefKind::Local, "another-branch", "head"),
                reference(RefKind::Local, "feature/topic", "head"),
                reference(RefKind::Remote, "fork/review/feature/topic", "head"),
                reference(RefKind::Tag, "v1", "head"),
            ],
        ));
        let badges = &groups["head"];
        assert_eq!(badges.len(), 3);
        assert_eq!(badges[0].name, "feature/topic");
        assert!(badges[0].checked_out);
        assert_eq!(
            badges[0].icons().collect::<Vec<_>>(),
            [RefKind::Local, RefKind::Remote]
        );
        assert_eq!(
            badges[0]
                .remote_refs
                .iter()
                .map(|reference| reference.name.as_str())
                .collect::<Vec<_>>(),
            ["origin/feature/topic", "fork/review/feature/topic"]
        );
        assert!(!badges[1].checked_out);
        assert_eq!(badges[2].icons().collect::<Vec<_>>(), [RefKind::Tag]);
    }

    #[test]
    fn diverged_refs_show_on_the_commits_they_point_to() {
        let groups = group_refs(&snapshot(
            Some("main"),
            vec![
                reference(RefKind::Local, "main", "head"),
                reference(RefKind::Remote, "origin/main", "old"),
            ],
        ));
        assert!(groups["head"][0].checked_out);
        assert_eq!(
            groups["head"][0].icons().collect::<Vec<_>>(),
            [RefKind::Local]
        );
        let remote = &groups["old"][0];
        assert_eq!(remote.display_name(), "main");
        assert_eq!(remote.tooltip(), "origin/main");
        assert!(!remote.checked_out);
        assert_eq!(remote.icons().collect::<Vec<_>>(), [RefKind::Remote]);
    }

    #[test]
    fn detached_head_gets_a_checked_badge_without_a_branch() {
        let groups = group_refs(&snapshot(None, Vec::new()));
        assert_eq!(groups["head"][0].name, "HEAD");
        assert!(groups["head"][0].checked_out);
        assert_eq!(groups["head"][0].icons().count(), 0);
    }
}
