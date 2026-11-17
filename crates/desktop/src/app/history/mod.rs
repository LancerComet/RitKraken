mod branch_editor;
mod commit_menu;
mod graph;
mod labels;
mod layout;
pub(in crate::app) mod rebase;
mod search;
mod view;

use std::{collections::HashMap, sync::Arc};

use gpui_kit::*;
use ritkraken_git::{CommitMenuSnapshot, Repo, Snapshot};

use super::{Client, Load, Message, changes::ChangeSummary, widgets::table::ResizableColumns};
pub(super) use branch_editor::BranchEditor;
use labels::{RefBadge, group_refs};
use layout::GraphLayout;

pub(super) struct History {
    pub snapshot: Snapshot,
    pub position: HashMap<String, usize>,
    pub(super) menu: Arc<CommitMenuSnapshot>,
    pub(super) worktree_summary: ChangeSummary,
    commits_graph: GraphLayout,
    worktree_graph: GraphLayout,
    labels: HashMap<String, Arc<[RefBadge]>>,
}

impl History {
    pub(super) fn projected(
        snapshot: Snapshot,
        menu: Arc<CommitMenuSnapshot>,
        worktree_summary: ChangeSummary,
    ) -> Arc<Self> {
        let commits_graph = GraphLayout::new(&snapshot.commits);
        let worktree_graph =
            GraphLayout::with_worktree(&snapshot.commits, snapshot.head.as_deref());
        let position = snapshot
            .commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (commit.id.clone(), index))
            .collect();
        let labels = group_refs(&snapshot);
        Arc::new(Self {
            snapshot,
            menu,
            position,
            worktree_summary,
            commits_graph,
            worktree_graph,
            labels,
        })
    }
}

impl History {
    pub(super) fn load(repo: &Repo, limit: Option<usize>) -> ritkraken_git::Result<Arc<Self>> {
        Self::reload(repo, limit, None)
    }

    pub(super) fn reload(
        repo: &Repo,
        limit: Option<usize>,
        previous: Option<&Arc<Self>>,
    ) -> ritkraken_git::Result<Arc<Self>> {
        let snapshot = repo.snapshot(limit)?;
        if let Some(previous) = previous.filter(|previous| previous.snapshot == snapshot) {
            return Ok(previous.clone());
        }
        let menu = repo.commit_menu_snapshot(&snapshot, previous.map(|history| &history.menu))?;
        let worktree_summary = ChangeSummary::new(snapshot.status.distinct_changes());
        Ok(Self::projected(snapshot, menu, worktree_summary))
    }

    fn labels_for(&self, id: &str) -> Option<&Arc<[RefBadge]>> {
        self.labels.get(id).filter(|labels| !labels.is_empty())
    }

    fn graph(&self, show_wip: bool) -> &GraphLayout {
        if show_wip {
            &self.worktree_graph
        } else {
            &self.commits_graph
        }
    }
}

pub(super) struct HistoryState {
    pub(super) data: Arc<History>,
    pub(super) selected: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) scroll: UniformListScrollHandle,
    horizontal_scroll: ScrollHandle,
    columns: ResizableColumns,
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
            columns: ResizableColumns::new(cx),
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
        if self
            .branch_editor
            .for_tab(id)
            .is_some_and(|(target, _)| Some(target.as_str()) != selected.as_deref())
        {
            self.branch_editor.clear();
        }
        let Some(tab) = self.tab_mut(id) else { return };
        tab.history.selected = selected;
        tab.review.close();
        let request = tab.changes.select_commit();
        let show_wip = tab.show_wip();
        let (index, commit, has_selected) = {
            let history = tab.active_history();
            let index = tab
                .history
                .selected
                .as_ref()
                .and_then(|oid| history.position.get(oid).copied());
            let commit = tab
                .history
                .selected
                .as_deref()
                .and_then(|sel| {
                    tab.rebase.as_ref().and_then(|session| {
                        session
                            .meta_for(sel)
                            .and_then(|meta| session.plan.entries.get(meta.entry_index))
                            .map(|entry| entry.commit.clone())
                    })
                })
                .or_else(|| index.map(|idx| history.snapshot.commits[idx].clone()));
            (index, commit, tab.history.selected.is_some())
        };
        if let Some(scroll) = scroll {
            tab.history.scroll.scroll_to_item(
                index.map_or(0, |index| index + usize::from(show_wip)),
                scroll,
            );
        }
        if let Some(commit) = commit {
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
        } else if has_selected {
            tab.changes.detail = Load::Failed(Message::Key("app.error.ref_outside_history"));
        }
        self.sync_rebase_editor_for_selection(id, window, cx);
        cx.notify();
    }

    pub(super) fn move_selection(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self
            .rebase_modal
            .as_ref()
            .map(|dialog| &dialog.tab)
            .or_else(|| self.current())
        else {
            return;
        };
        let offset = usize::from(tab.show_wip());
        let history = tab.active_history();
        let count = history.snapshot.commits.len() + offset;
        let position = tab
            .history
            .selected
            .as_ref()
            .and_then(|oid| history.position.get(oid))
            .map_or(0, |index| *index + offset);
        let index = position
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        let selected = index
            .checked_sub(offset)
            .and_then(|index| history.snapshot.commits.get(index))
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
                && tab.rebase.is_none()
                && tab.review.active.is_none()
                && !tab.busy
                && tab.history.data.snapshot.history_limited
                && !tab.notice.as_ref().is_some_and(super::Notice::is_error)
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
