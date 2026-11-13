mod restore;

use std::{path::Path, sync::Arc};

use gpui_kit::*;
use ritkraken_git::Repo;

use super::{
    Client, Message,
    changes::ChangesState,
    history::{History, HistoryState},
    operations::RepoUpdates,
    refs::ReferenceState,
    review::ReviewState,
};

#[derive(Default)]
pub(super) struct Session {
    pub(super) tabs: Vec<Tab>,
    pub(super) active: Option<u64>,
    next_tab: u64,
    pub(super) opening: bool,
}

impl Session {
    fn next_id(&mut self) -> u64 {
        let id = self.next_tab;
        self.next_tab += 1;
        id
    }

    pub(super) fn active_tab(&self) -> Option<&Tab> {
        self.entry(self.active?)
    }

    pub(super) fn entry(&self, id: u64) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub(super) fn entry_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }
}

pub(super) enum Tab {
    Saved(restore::SavedTab),
    Ready(Box<RepoTab>),
}

impl Tab {
    pub(super) fn id(&self) -> u64 {
        match self {
            Self::Saved(tab) => tab.id,
            Self::Ready(tab) => tab.id,
        }
    }

    pub(super) fn root(&self) -> &Path {
        match self {
            Self::Saved(tab) => &tab.path,
            Self::Ready(tab) => tab.repo.root(),
        }
    }

    pub(super) fn name(&self) -> String {
        Repo::name_from_path(self.root())
    }

    pub(super) fn ready(&self) -> Option<&RepoTab> {
        match self {
            Self::Ready(tab) => Some(tab),
            Self::Saved(_) => None,
        }
    }

    pub(super) fn ready_mut(&mut self) -> Option<&mut RepoTab> {
        match self {
            Self::Ready(tab) => Some(tab),
            Self::Saved(_) => None,
        }
    }

    pub(super) fn can_close(&self) -> bool {
        !self.ready().is_some_and(|tab| tab.busy)
    }

    pub(super) fn is_busy(&self) -> bool {
        match self {
            Self::Saved(tab) => tab.is_loading(),
            Self::Ready(tab) => tab.busy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveOp {
    Fetch,
    Push,
    Branch,
    Stash,
    Pop,
    Commit,
    Other,
}

pub(super) struct RepoTab {
    pub(super) id: u64,
    pub(super) repo: Repo,
    pub(super) history: HistoryState,
    pub(super) refs: ReferenceState,
    pub(super) changes: ChangesState,
    pub(super) review: ReviewState,
    pub(super) rebase: Option<super::history::rebase::RebaseSession>,
    pub(super) busy: bool,
    pub(super) active_op: Option<ActiveOp>,
    pub(super) updates: RepoUpdates,
    pub(super) notice: Option<(bool, Message)>,
}

impl RepoTab {
    pub(super) fn active_history(&self) -> &std::sync::Arc<super::history::History> {
        self.rebase
            .as_ref()
            .map_or(&self.history.data, |session| &session.projected.history)
    }

    pub(super) fn has_changes(&self) -> bool {
        !self.history.data.snapshot.status.is_empty()
    }

    pub(super) fn show_wip(&self) -> bool {
        self.rebase.is_none() && (self.has_changes() || self.history.selected.is_none())
    }
}

impl Client {
    pub(super) fn show_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.entry(id).is_none() {
            return;
        }
        self.session.active = Some(id);
        self.branch_editor.clear();
        self.dismiss_modal(window, cx);
        self.load_saved_tab(id, window, cx);
        self.select_initial_commit(id, window, cx);
        self.watch_current_repo(window, cx);
        self.persist_session();
        cx.notify();
    }

    fn select_initial_commit(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab(id).filter(|tab| tab.changes.request == 0) {
            self.select(
                id,
                tab.history.selected.clone(),
                Some(ScrollStrategy::Top),
                window,
                cx,
            );
        }
    }

    pub(super) fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .session
            .tabs
            .iter()
            .position(|tab| tab.id() == id && tab.can_close())
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
                self.show_tab(tab.id(), window, cx);
                return;
            }
            self.session.active = None;
            self.watch_current_repo(window, cx);
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
                .map(|tab| tab.root().to_path_buf())
                .collect();
            self.saved.active_tab = self
                .session
                .active_tab()
                .map(|tab| tab.root().to_path_buf());
        } else {
            self.saved.open_tabs.clear();
            self.saved.active_tab = None;
        }
        self.saved.save();
    }

    pub(super) fn insert_tab(
        &mut self,
        repo: Repo,
        history: Arc<History>,
        limit: Option<usize>,
        saved_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        let existing = self
            .session
            .tabs
            .iter()
            .find(|tab| tab.root() == repo.root())
            .map(Tab::id);
        let id = existing
            .or(saved_id)
            .unwrap_or_else(|| self.session.next_id());
        if let Some(duplicate) = saved_id.filter(|&saved| saved != id) {
            self.session.tabs.retain(|tab| tab.id() != duplicate);
            if self.session.active == Some(duplicate) {
                self.session.active = Some(id);
            }
        }
        if self.tab(id).is_some() {
            return id;
        }
        let ready = Tab::Ready(Box::new(RepoTab {
            id,
            repo,
            history: HistoryState::new(history, limit, cx),
            refs: ReferenceState::new(window, cx),
            changes: ChangesState::new(window, cx),
            review: ReviewState::new(&self.saved.preferences),
            rebase: None,
            busy: false,
            active_op: None,
            updates: RepoUpdates::default(),
            notice: None,
        }));
        if let Some(tab) = self.session.entry_mut(id) {
            *tab = ready;
        } else {
            self.session.tabs.push(ready);
        }
        id
    }
}
