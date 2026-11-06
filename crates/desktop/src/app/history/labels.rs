use std::collections::HashMap;

use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{RefKind, Snapshot};

use super::graph::{BAND_HEIGHT, CONNECTOR_OPACITY};
use crate::app::{
    refs::ref_icon,
    widgets::{TEXT, icon},
};

#[derive(Debug)]
pub(super) struct RefBadge {
    name: String,
    short_name: Option<String>,
    kind: Option<RefKind>,
    checked_out: bool,
    remote_refs: Vec<String>,
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
            .chain(&self.remote_refs)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

pub(super) fn group_refs(snapshot: &Snapshot) -> HashMap<String, Vec<RefBadge>> {
    let mut groups: HashMap<String, Vec<RefBadge>> = HashMap::new();
    let mut local_positions = HashMap::new();
    let head = snapshot.head.as_deref();

    for kind in [RefKind::Local, RefKind::Remote, RefKind::Tag] {
        for reference in snapshot.refs_of_kind(kind) {
            let badges = groups.entry(reference.id.clone()).or_default();
            let short_name = (kind == RefKind::Remote)
                .then(|| remote_branch_name(&reference.name, &snapshot.remotes))
                .flatten();
            if kind == RefKind::Local {
                local_positions.insert(
                    (reference.id.as_str(), reference.name.as_str()),
                    badges.len(),
                );
            } else if let Some(name) = short_name
                && let Some(&position) = local_positions.get(&(reference.id.as_str(), name))
            {
                badges[position].remote_refs.push(reference.name.clone());
                continue;
            }
            badges.push(RefBadge {
                name: reference.name.clone(),
                short_name: short_name.map(str::to_owned),
                kind: Some(kind),
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
                    checked_out: true,
                    remote_refs: Vec::new(),
                },
            );
        } else {
            badges.sort_by_key(|badge| !badge.checked_out);
        }
    }
    groups
}

fn remote_branch_name<'a>(name: &'a str, remotes: &[String]) -> Option<&'a str> {
    remotes
        .iter()
        .filter_map(|remote| {
            let branch = name.strip_prefix(remote)?.strip_prefix('/')?;
            Some((remote.len(), branch))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, branch)| branch)
        .or_else(|| name.split_once('/').map(|(_, branch)| branch))
}

pub(super) fn ref_labels(labels: &[RefBadge], color: u32) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .pl(px(7.))
        .gap(px(3.))
        .overflow_hidden()
        .when_some(labels.split_first(), |element, (first, rest)| {
            let tooltip = first.tooltip();
            element
                .child(
                    badge(color, if first.checked_out { 0.48 } else { 0.25 })
                        .id("commit-ref")
                        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
                        .when(first.checked_out, |e| {
                            e.child(icon("icons/check.svg", TEXT).size(px(12.)))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(first.display_name().to_owned()),
                        )
                        .children(
                            first
                                .icons()
                                .map(|kind| icon(ref_icon(kind), TEXT).size(px(12.))),
                        ),
                )
                .when(!rest.is_empty(), |element| {
                    let tooltip = rest
                        .iter()
                        .map(RefBadge::tooltip)
                        .collect::<Vec<_>>()
                        .join("\n");
                    element.child(
                        badge(color, 0.25)
                            .id("additional-refs")
                            .flex_shrink_0()
                            .tooltip(move |window, cx| {
                                Tooltip::new(tooltip.clone()).build(window, cx)
                            })
                            .child(format!("+{}", rest.len())),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .h(px(1.))
                        .bg(rgb(color).opacity(CONNECTOR_OPACITY)),
                )
        })
}

fn badge(color: u32, opacity: f32) -> Div {
    div()
        .min_w_0()
        .h(BAND_HEIGHT)
        .px(px(4.))
        .flex()
        .items_center()
        .gap(px(3.))
        .rounded(px(2.))
        .text_size(px(12.))
        .text_color(rgb(TEXT))
        .bg(rgb(color).opacity(opacity))
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
        }
    }

    fn snapshot(branch: Option<&str>, refs: Vec<Reference>) -> Snapshot {
        Snapshot {
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
            badges[0].remote_refs,
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
