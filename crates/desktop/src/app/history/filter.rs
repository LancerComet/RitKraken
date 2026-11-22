//! Per-repository graph branch filter: the persisted hide/solo ref store,
//! its side effects (sidebar markers, banner) and the toggle actions.

use std::collections::{BTreeMap, HashSet};

use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{HistoryFilter, Reference, Repo};

use crate::{
    app::{
        Client, RepoTab,
        settings::{read_state, write_state},
        theme::palette,
        widgets::{Tone, button, icon},
    },
    i18n::tr,
};

const FILE: &str = "graph_filters.json";

/// Filters by repository root path, persisted in `graph_filters.json`.
pub(in crate::app) type Store = BTreeMap<String, HistoryFilter>;

pub(in crate::app) fn root_key(repo: &Repo) -> String {
    repo.root().to_string_lossy().into_owned()
}

pub(in crate::app) fn load() -> Store {
    read_state(FILE)
        .map(|value| read_store(&value))
        .unwrap_or_default()
}

fn read_store(value: &serde_json::Value) -> Store {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(root, entry)| {
            let filter = read_filter(entry);
            filter.is_active().then(|| (root.clone(), filter))
        })
        .collect()
}

fn read_filter(value: &serde_json::Value) -> HistoryFilter {
    let names = |key: &str| {
        value[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|name| name.as_str().map(str::to_owned))
            .collect()
    };
    HistoryFilter {
        hidden: names("hidden"),
        solo: names("solo"),
    }
}

fn store_json(store: &Store) -> serde_json::Value {
    store
        .iter()
        .filter(|(_, filter)| filter.is_active())
        .map(|(root, filter)| {
            (
                root.clone(),
                serde_json::json!({
                    "hidden": filter.hidden,
                    "solo": filter.solo,
                }),
            )
        })
        .collect()
}

pub(in crate::app) fn save(store: &Store) {
    write_state(FILE, &store_json(store));
}

/// What pruning did to a filter.
pub(in crate::app) struct PruneResult {
    /// Entries were dropped; persist the new state.
    pub changed: bool,
    /// The solo set went from non-empty to empty: the graph switches from
    /// solo to hidden/default mode, so the shown history is stale and must
    /// be re-read.
    pub solo_emptied: bool,
}

/// Drops filter entries whose reference no longer exists.
pub(in crate::app) fn prune(filter: &mut HistoryFilter, refs: &[Reference]) -> PruneResult {
    let names: HashSet<String> = refs.iter().map(Reference::full_name).collect();
    let soloed = !filter.solo.is_empty();
    let before = filter.hidden.len() + filter.solo.len();
    filter.hidden.retain(|name| names.contains(name));
    filter.solo.retain(|name| names.contains(name));
    PruneResult {
        changed: before != filter.hidden.len() + filter.solo.len(),
        solo_emptied: soloed && filter.solo.is_empty(),
    }
}

/// Why a filter change is unavailable right now, if it is: the graph reload
/// it triggers shares the read-only update path.
pub(in crate::app) fn blocked_reason(tab: &RepoTab) -> Option<String> {
    if tab.busy {
        Some(tr!("app.commit_menu.busy"))
    } else if tab.rebase.is_some() {
        Some(tr!("app.rebase.planning_active"))
    } else {
        None
    }
}

fn short_ref_name(full: &str) -> &str {
    full.strip_prefix("refs/heads/")
        .or_else(|| full.strip_prefix("refs/remotes/"))
        .or_else(|| full.strip_prefix("refs/tags/"))
        .unwrap_or(full)
}

impl Client {
    pub(in crate::app) fn remember_graph_filter(&mut self, root: &str, filter: HistoryFilter) {
        if filter.is_active() {
            self.graph_filters.insert(root.to_owned(), filter);
        } else {
            self.graph_filters.remove(root);
        }
        save(&self.graph_filters);
    }

    fn apply_graph_filter(
        &mut self,
        id: u64,
        change: impl FnOnce(&mut HistoryFilter),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        if blocked_reason(tab).is_some() {
            return;
        }
        change(&mut tab.history.filter);
        let filter = tab.history.filter.clone();
        let root = root_key(&tab.repo);
        self.remember_graph_filter(&root, filter);
        self.update_repo(id, None, window, cx);
    }

    pub(in crate::app) fn toggle_graph_hidden(
        &mut self,
        id: u64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_graph_filter(
            id,
            |filter| {
                if !filter.hidden.remove(&name) {
                    filter.hide(&name);
                }
            },
            window,
            cx,
        );
    }

    pub(in crate::app) fn toggle_graph_solo(
        &mut self,
        id: u64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_graph_filter(
            id,
            |filter| {
                if !filter.solo.remove(&name) {
                    filter.solo(&name);
                }
            },
            window,
            cx,
        );
    }

    pub(in crate::app) fn clear_graph_filter(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_graph_filter(id, HistoryFilter::clear, window, cx);
    }
}

/// The slim bar above the graph's column header while a filter is active.
pub(in crate::app) fn filter_banner(tab: &RepoTab, cx: &mut Context<Client>) -> Option<Div> {
    let filter = &tab.history.filter;
    if !filter.is_active() {
        return None;
    }
    let p = palette(cx);
    let id = tab.id;
    let text = if filter.solo.is_empty() {
        tr!("app.history.refs_hidden", count = filter.hidden.len())
    } else {
        let names = filter
            .solo
            .iter()
            .map(|name| short_ref_name(name))
            .collect::<Vec<_>>()
            .join(", ");
        tr!("app.history.showing_only", names = names)
    };
    let reason = blocked_reason(tab);
    Some(
        div()
            .w_full()
            .flex_shrink_0()
            .bg(rgb(p.surface_high))
            .border_b_1()
            .border_color(rgb(p.border))
            .px(px(12.))
            .py(px(5.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.5))
                    .text_color(rgb(p.text_muted))
                    .child(icon("icons/eye-off.svg", p.text_muted).size(px(13.)))
                    .child(div().truncate().child(text)),
            )
            .child(
                button(
                    ("graph-filter-clear", id),
                    tr!("app.refs.show_all"),
                    Tone::Plain,
                    reason.is_none(),
                    cx,
                    cx.listener(move |this, _, window, cx| {
                        this.clear_graph_filter(id, window, cx);
                    }),
                )
                .rounded_full()
                .h(px(22.))
                .px(px(10.))
                .text_size(px(11.))
                .when_some(reason, |element, reason| {
                    element
                        .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                }),
            ),
    )
}

#[cfg(test)]
mod tests {
    use ritkraken_git::{HistoryFilter, RefKind, Reference};

    use super::{Store, prune, read_store, store_json};

    fn refs(names: &[&str]) -> Vec<Reference> {
        names
            .iter()
            .map(|name| {
                let (kind, name) = name
                    .strip_prefix("refs/heads/")
                    .map(|name| (RefKind::Local, name))
                    .or_else(|| {
                        name.strip_prefix("refs/remotes/")
                            .map(|name| (RefKind::Remote, name))
                    })
                    .unwrap_or_else(|| (RefKind::Tag, name.strip_prefix("refs/tags/").unwrap()));
                Reference {
                    kind,
                    name: name.to_owned(),
                    id: String::new(),
                    object_id: String::new(),
                    checked_out: false,
                }
            })
            .collect()
    }

    #[test]
    fn toggles_follow_the_hide_solo_rules() {
        let mut filter = HistoryFilter::default();
        filter.solo("refs/heads/a");
        filter.hide("refs/heads/a");
        assert!(filter.hidden.contains("refs/heads/a"));
        assert!(!filter.solo.contains("refs/heads/a"));
        filter.show("refs/heads/a");
        assert!(!filter.is_active());
        filter.hide("refs/heads/b");
        filter.solo("refs/heads/b");
        assert!(filter.solo.contains("refs/heads/b"));
        assert!(!filter.hidden.contains("refs/heads/b"));
        filter.unsolo("refs/heads/b");
        filter.hide("refs/heads/c");
        assert!(filter.hidden.contains("refs/heads/c"));
        filter.clear();
        assert!(!filter.is_active());
    }

    #[test]
    fn exclusion_gives_solo_precedence_over_hidden() {
        let mut filter = HistoryFilter::default();
        filter.hidden.insert("refs/heads/a".to_owned());
        assert!(filter.is_excluded("refs/heads/a"));
        assert!(!filter.is_excluded("refs/heads/b"));
        filter.solo.insert("refs/heads/b".to_owned());
        assert!(!filter.is_excluded("refs/heads/b"));
        assert!(filter.is_excluded("refs/heads/a"));
        assert!(filter.is_excluded("refs/heads/c"));
    }

    #[test]
    fn store_round_trips_and_omits_empty_filters() {
        let mut store = Store::new();
        let mut filter = HistoryFilter::default();
        filter.hide("refs/heads/a");
        filter.solo("refs/tags/v1");
        store.insert("/repo/one".to_owned(), filter.clone());
        store.insert("/repo/empty".to_owned(), HistoryFilter::default());
        let json = store_json(&store);
        assert!(json.get("/repo/empty").is_none());
        assert_eq!(read_store(&json)["/repo/one"], filter);
    }

    #[test]
    fn pruning_drops_missing_refs_and_keeps_existing() {
        let mut filter = HistoryFilter::default();
        filter.hidden.insert("refs/heads/gone".to_owned());
        filter.hidden.insert("refs/heads/main".to_owned());
        let refs = refs(&["refs/heads/main", "refs/remotes/origin/main"]);
        let pruned = prune(&mut filter, &refs);
        assert!(pruned.changed && !pruned.solo_emptied);
        assert_eq!(filter.hidden.len(), 1);
        assert!(filter.hidden.contains("refs/heads/main"));
        assert!(!prune(&mut filter, &refs).changed);
    }

    #[test]
    fn pruning_reports_when_the_solo_set_empties() {
        let refs = refs(&["refs/heads/main"]);
        // One of two solo refs survives: same solo mode, no re-read needed.
        let mut filter = HistoryFilter::default();
        filter.solo.insert("refs/heads/main".to_owned());
        filter.solo.insert("refs/heads/gone".to_owned());
        let pruned = prune(&mut filter, &refs);
        assert!(pruned.changed && !pruned.solo_emptied);
        // The last solo ref drops: the graph switches modes, caller re-reads.
        let mut filter = HistoryFilter::default();
        filter.solo.insert("refs/heads/gone".to_owned());
        let pruned = prune(&mut filter, &refs);
        assert!(pruned.changed && pruned.solo_emptied);
        // Hidden-only pruning can never empty a solo set.
        let mut filter = HistoryFilter::default();
        filter.hidden.insert("refs/heads/gone".to_owned());
        let pruned = prune(&mut filter, &refs);
        assert!(pruned.changed && !pruned.solo_emptied);
        // Nothing dropped.
        let mut filter = HistoryFilter::default();
        filter.hidden.insert("refs/heads/main".to_owned());
        let pruned = prune(&mut filter, &refs);
        assert!(!pruned.changed && !pruned.solo_emptied);
    }
}
