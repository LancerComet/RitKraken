mod graph;
mod labels;
mod layout;
mod view;

use std::{collections::HashMap, sync::Arc};

use gpui_kit::*;
use ritkraken_git::{Repo, Snapshot};

use super::{Client, Load, Message, changes::ChangeSummary};
use labels::{RefBadge, group_refs};
use layout::GraphLayout;

pub(super) struct History {
    pub snapshot: Snapshot,
    pub position: HashMap<String, usize>,
    pub(super) worktree_summary: ChangeSummary,
    graph: GraphLayout,
    labels: HashMap<String, Arc<[RefBadge]>>,
}

impl History {
    pub(super) fn load(repo: &Repo, limit: Option<usize>) -> ritkraken_git::Result<Arc<Self>> {
        let snapshot = repo.snapshot(limit)?;
        let graph = GraphLayout::new(&snapshot.commits);
        let worktree_summary = ChangeSummary::new(snapshot.status.distinct_changes());
        let position = snapshot
            .commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (commit.id.clone(), index))
            .collect();
        let labels = group_refs(&snapshot);
        Ok(Arc::new(Self {
            snapshot,
            position,
            worktree_summary,
            graph,
            labels,
        }))
    }

    fn labels_for(&self, id: &str) -> Option<&Arc<[RefBadge]>> {
        self.labels.get(id).filter(|labels| !labels.is_empty())
    }

    fn head_index(&self) -> Option<usize> {
        self.position.get(self.snapshot.head.as_deref()?).copied()
    }
}

pub(super) struct HistoryState {
    pub(super) data: Arc<History>,
    pub(super) selected: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) scroll: UniformListScrollHandle,
    horizontal_scroll: ScrollHandle,
    pub(super) focus: FocusHandle,
}

impl HistoryState {
    pub(super) fn new(data: Arc<History>, limit: Option<usize>, cx: &mut Context<Client>) -> Self {
        Self {
            selected: Self::initial_selection(&data),
            data,
            limit,
            scroll: UniformListScrollHandle::default(),
            horizontal_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    pub(super) fn initial_selection(history: &History) -> Option<String> {
        if history.snapshot.status.is_empty() {
            history.snapshot.head.clone()
        } else {
            None
        }
    }
}

impl Client {
    pub(super) fn select(
        &mut self,
        id: u64,
        selected: Option<String>,
        scroll: Option<ScrollStrategy>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.history.selected = selected;
        tab.review.close();
        let request = tab.changes.select_commit();
        let index = tab
            .history
            .selected
            .as_ref()
            .and_then(|oid| tab.history.data.position.get(oid).copied());
        if let Some(scroll) = scroll {
            tab.history.scroll.scroll_to_item(
                index.map_or(0, |index| index + usize::from(tab.show_wip())),
                scroll,
            );
        }
        if let Some(index) = index {
            let commit = tab.history.data.snapshot.commits[index].clone();
            let repo = tab.repo.clone();
            Self::spawn_background(
                window,
                cx,
                move || repo.detail(&commit),
                move |this, result, _, cx| {
                    if let Some(changes) = this.changes_for_request(id, request) {
                        changes.detail = result.into();
                        cx.notify();
                    }
                },
            );
        } else if tab.history.selected.is_some() {
            tab.changes.detail = Load::Failed(Message::Key("app.error.ref_outside_history"));
        }
        cx.notify();
    }

    pub(super) fn move_selection(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.current() else { return };
        let offset = usize::from(tab.show_wip());
        let count = tab.history.data.snapshot.commits.len() + offset;
        let position = tab
            .history
            .selected
            .as_ref()
            .and_then(|oid| tab.history.data.position.get(oid))
            .map_or(0, |index| *index + offset);
        let index = position
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        let selected = index
            .checked_sub(offset)
            .and_then(|index| tab.history.data.snapshot.commits.get(index))
            .map(|commit| commit.id.clone());
        self.select(tab.id, selected, Some(ScrollStrategy::Nearest), window, cx);
    }

    pub(super) fn load_older(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let prefs = &self.saved.preferences;
        if !prefs.lazy_history
            || prefs.show_all_commits
            || self.preferences.is_some()
            || self.manager.mode.is_some()
        {
            return;
        }
        let Some(tab) = self.current().filter(|tab| {
            tab.id == id
                && tab.review.active.is_none()
                && !tab.busy
                && tab.history.data.snapshot.history_limited
                && !tab.notice.as_ref().is_some_and(|(error, _)| *error)
        }) else {
            return;
        };
        if let Some(limit) = tab.history.limit {
            self.update_repo_with_limit(
                id,
                None,
                Some(limit.saturating_add(prefs.initial_commits)),
                window,
                cx,
            );
        }
    }
}
