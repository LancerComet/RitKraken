use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::Repo;

use super::{
    Client,
    changes::ChangesState,
    history::{History, HistoryState},
    refs::ReferenceState,
    review::ReviewState,
};

#[derive(Default)]
pub(super) struct Session {
    pub(super) tabs: Vec<RepoTab>,
    pub(super) active: Option<u64>,
    pub(super) next_tab: u64,
    pub(super) opening: bool,
}

pub(super) struct RepoTab {
    pub(super) id: u64,
    pub(super) repo: Repo,
    pub(super) history: HistoryState,
    pub(super) refs: ReferenceState,
    pub(super) changes: ChangesState,
    pub(super) review: ReviewState,
    pub(super) busy: bool,
    pub(super) notice: Option<(bool, String)>,
}

impl RepoTab {
    pub(super) fn has_changes(&self) -> bool {
        !self.history.data.snapshot.status.is_empty()
    }

    pub(super) fn show_wip(&self) -> bool {
        self.has_changes() || self.history.selected.is_none()
    }
}

impl Client {
    pub(super) fn show_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tab(id) else { return };
        let first_selection = (tab.changes.request == 0).then(|| tab.history.selected.clone());
        self.session.active = Some(id);
        self.manager.mode = None;
        self.preferences = None;
        self.branch_dialog.open = false;
        self.focus.focus(window, cx);
        if let Some(selection) = first_selection {
            self.select(id, selection, Some(ScrollStrategy::Top), window, cx);
        }
        self.persist_session();
        cx.notify();
    }

    pub(super) fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .session
            .tabs
            .iter()
            .position(|tab| tab.id == id && !tab.busy)
        else {
            return;
        };
        self.session.tabs.remove(index);
        if self.session.active == Some(id) {
            if let Some(tab) = self
                .session
                .tabs
                .get(index.min(self.session.tabs.len().saturating_sub(1)))
            {
                self.show_tab(tab.id, window, cx);
                return;
            }
            self.session.active = None;
            self.focus.focus(window, cx);
        }
        self.persist_session();
        cx.notify();
    }

    pub(super) fn persist_session(&mut self) {
        if self.saved.preferences.remember_tabs {
            self.saved.open_tabs = self
                .session
                .tabs
                .iter()
                .map(|tab| tab.repo.root().to_path_buf())
                .collect();
            self.saved.active_tab = self.current().map(|tab| tab.repo.root().to_path_buf());
        } else {
            self.saved.open_tabs.clear();
            self.saved.active_tab = None;
        }
        self.saved.save();
    }

    pub(super) fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.opening = true;
        let paths = self.saved.open_tabs.clone();
        let active = self.saved.active_tab.clone();
        let limit = self.saved.preferences.history_limit();
        Self::spawn_background(
            window,
            cx,
            move || {
                paths
                    .into_iter()
                    .filter_map(|path| {
                        let repo = Repo::open(&path).ok()?;
                        let history = History::load(&repo, limit).ok()?;
                        Some((repo, history))
                    })
                    .collect::<Vec<_>>()
            },
            move |this, tabs, window, cx| {
                this.session.opening = false;
                let mut selected = None;
                for (repo, history) in tabs {
                    let was_active = active.as_deref() == Some(repo.root());
                    let id = this.insert_tab(repo, history, limit, window, cx);
                    if was_active || selected.is_none() {
                        selected = Some(id);
                    }
                }
                if let Some(id) = selected {
                    this.show_tab(id, window, cx);
                } else {
                    this.manager.error =
                        Some("Previously opened repositories are unavailable.".into());
                    this.persist_session();
                    cx.notify();
                }
            },
        );
    }

    pub(super) fn insert_tab(
        &mut self,
        repo: Repo,
        history: Arc<History>,
        limit: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        if let Some(tab) = self
            .session
            .tabs
            .iter()
            .find(|tab| tab.repo.root() == repo.root())
        {
            return tab.id;
        }
        let id = self.session.next_tab;
        self.session.next_tab += 1;
        self.session.tabs.push(RepoTab {
            id,
            repo,
            history: HistoryState::new(history, limit, cx),
            refs: ReferenceState::new(window, cx),
            changes: ChangesState::new(window, cx),
            review: ReviewState::new(&self.saved.preferences),
            busy: false,
            notice: None,
        });
        id
    }
}
