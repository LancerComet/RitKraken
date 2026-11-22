mod branch_editor;
mod commit_menu;
pub(super) mod filter;
mod graph;
mod labels;
mod layout;
pub(in crate::app) mod rebase;
mod search;
mod stash_menu;
mod view;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use gpui_kit::*;
use ritkraken_git::{CommitMenuSnapshot, HistoryFilter, Repo, Snapshot};

use super::{
    Client, Load, Message, changes::ChangeSummary, session::Tab,
    widgets::table::ResizableColumns,
};
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
        let stashes: HashSet<&str> = snapshot
            .stashes
            .iter()
            .map(|stash| stash.id.as_str())
            .collect();
        let commits_graph = GraphLayout::new(&snapshot.commits, &stashes);
        let worktree_graph =
            GraphLayout::with_worktree(&snapshot.commits, snapshot.head.as_deref(), &stashes);
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
    pub(super) fn load(
        repo: &Repo,
        limit: Option<usize>,
        filter: &HistoryFilter,
    ) -> ritkraken_git::Result<Arc<Self>> {
        Self::reload(repo, limit, None, false, filter)
    }

    pub(super) fn reload(
        repo: &Repo,
        limit: Option<usize>,
        previous: Option<&Arc<Self>>,
        page_only: bool,
        filter: &HistoryFilter,
    ) -> ritkraken_git::Result<Arc<Self>> {
        let snapshot = match previous.filter(|_| page_only) {
            Some(previous) => repo.snapshot_page(&previous.snapshot, limit, filter)?,
            None => repo.snapshot_filtered(limit, filter)?,
        };
        if let Some(previous) = previous.filter(|previous| previous.snapshot == snapshot) {
            return Ok(previous.clone());
        }
        let menu = repo.commit_menu_snapshot(&snapshot, previous.map(|history| &history.menu))?;
        let worktree_summary = ChangeSummary::new(snapshot.status.distinct_changes());
        Ok(Self::projected(snapshot, menu, worktree_summary))
    }

    pub(super) fn stash(&self, id: &str) -> Option<&ritkraken_git::Stash> {
        self.snapshot.stashes.iter().find(|stash| stash.id == id)
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
    pub(super) filter: HistoryFilter,
    pub(super) selected: Option<String>,
    pub(super) limit: Option<usize>,
    /// The last row was painted but growing the history was blocked; retried
    /// when the next repository update finishes.
    pub(super) wants_older: bool,
    pub(super) scroll: UniformListScrollHandle,
    horizontal_scroll: ScrollHandle,
    columns: ResizableColumns,
    pub(super) focus: FocusHandle,
}

impl HistoryState {
    pub(super) fn new(
        data: Arc<History>,
        limit: Option<usize>,
        filter: HistoryFilter,
        cx: &mut Context<Client>,
    ) -> Self {
        Self {
            selected: Self::initial_selection(&data),
            data,
            filter,
            limit,
            wants_older: false,
            scroll: UniformListScrollHandle::default(),
            horizontal_scroll: ScrollHandle::new(),
            columns: ResizableColumns::new("graph-columns", cx),
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
        if !prefs.lazy_history || prefs.show_all_commits || self.session.active != Some(id) {
            return;
        }
        let Some(tab) = self.session.entry_mut(id).and_then(Tab::ready_mut) else {
            return;
        };
        if tab.rebase.is_some() || tab.review.active.is_some() {
            return;
        }
        if !tab.history.data.snapshot.history_limited {
            tab.history.wants_older = false;
            return;
        }
        // A painted row kept by the list's layer does not render again to
        // retrigger this; finish_update retries once the blocker clears. A
        // relaunch while a read runs would supersede and discard it, so the
        // repaint storm before it lands starves the expansion instead.
        if self.preferences.is_some()
            || self.manager.mode.is_some()
            || tab.busy
            || tab.updates.in_flight()
        {
            tab.history.wants_older = true;
            return;
        }
        tab.history.wants_older = false;
        if let Some(limit) = tab.history.limit.map(|limit| limit.saturating_mul(2)) {
            self.page_repo_history(id, limit, window, cx);
        }
    }
}
