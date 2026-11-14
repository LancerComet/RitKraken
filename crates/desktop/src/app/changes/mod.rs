mod commit;
mod commit_people;
mod files;
mod inspect;
mod menu;
mod summary;
mod worktree;

use gpui_kit::{
    component::input::{InputEvent, InputState, TextareaMode, TextareaState},
    prelude::*,
    *,
};
use ritkraken_git::{Change, CommitDetail, Scope};

use crate::i18n::{LocalizedInput, tr};

use super::{Client, Load, RepoOperation, RepoTab, theme::palette, widgets::Tone};

pub(super) use summary::{ChangeSummary, SummaryStyle, change_mark};

#[derive(Clone, Copy)]
pub(super) enum StageAction {
    Stage,
    Unstage,
}

pub(super) enum StageTarget {
    Row,
    File,
    All,
    Hunk,
    Line,
}

impl StageAction {
    pub(super) fn for_scope(scope: &Scope) -> Option<Self> {
        match scope {
            Scope::Worktree => Some(Self::Stage),
            Scope::Index => Some(Self::Unstage),
            Scope::Commit { .. } => None,
        }
    }

    pub(super) fn label(self, target: StageTarget) -> String {
        tr!(match (self, target) {
            (Self::Stage, StageTarget::Row) => "app.stage.stage",
            (Self::Unstage, StageTarget::Row) => "app.stage.unstage",
            (Self::Stage, StageTarget::File) => "app.stage.stage_file",
            (Self::Unstage, StageTarget::File) => "app.stage.unstage_file",
            (Self::Stage, StageTarget::All) => "app.stage.stage_all",
            (Self::Unstage, StageTarget::All) => "app.stage.unstage_all",
            (Self::Stage, StageTarget::Hunk) => "app.stage.stage_hunk",
            (Self::Unstage, StageTarget::Hunk) => "app.stage.unstage_hunk",
            (Self::Stage, StageTarget::Line) => "app.stage.stage_line",
            (Self::Unstage, StageTarget::Line) => "app.stage.unstage_line",
        })
    }

    pub(super) fn tone(self) -> Tone {
        match self {
            Self::Stage => Tone::Green,
            Self::Unstage => Tone::Red,
        }
    }
}

pub(super) struct ChangesState {
    pub(super) detail: Load<CommitDetail>,
    show_all_files: bool,
    commit_files: Option<Load<Vec<Change>>>,
    pub(super) request: u64,
    scroll: [UniformListScrollHandle; 3],
    pub(super) summary: LocalizedInput,
    pub(super) description: LocalizedInput<TextareaMode>,
    tree: bool,
    worktree_panes: Entity<gpui_kit::base::ResizableState>,
    message_panes: Entity<gpui_kit::base::ResizableState>,
    _summary: Subscription,
}

impl ChangesState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let summary =
            LocalizedInput::new("app.changes.summary", cx, |cx| InputState::new(window, cx));
        let description = LocalizedInput::new("app.changes.description", cx, |cx| {
            TextareaState::new(window, cx).rows(1)
        });
        let subscription = cx.subscribe_in(
            &summary,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter {
                    secondary: true, ..
                } => this.submit(window, cx),
                _ => {}
            },
        );
        Self {
            detail: Load::Pending,
            show_all_files: false,
            commit_files: None,
            request: 0,
            scroll: std::array::from_fn(|_| UniformListScrollHandle::default()),
            summary,
            description,
            tree: false,
            worktree_panes: cx.new(|_| gpui_kit::base::ResizableState::default()),
            message_panes: cx.new(|_| gpui_kit::base::ResizableState::default()),
            _summary: subscription,
        }
    }

    pub(super) fn select_commit(&mut self) -> u64 {
        self.detail = Load::Pending;
        self.show_all_files = false;
        self.commit_files = None;
        self.scroll[2] = UniformListScrollHandle::default();
        self.request += 1;
        self.request
    }
}

impl RepoTab {
    fn commit_enabled(&self, cx: &App) -> bool {
        self.history.selected.is_none()
            && !self.busy
            && !self.history.data.snapshot.status.staged.is_empty()
            && !self.changes.summary.read(cx).value().trim().is_empty()
    }
}

impl Client {
    pub(super) fn changes_for_request(
        &mut self,
        id: u64,
        request: u64,
    ) -> Option<&mut ChangesState> {
        let changes = &mut self.tab_mut(id)?.changes;
        (changes.request == request).then_some(changes)
    }

    pub(super) fn show_committed_files(
        &mut self,
        id: u64,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.changes.show_all_files = show;
        tab.changes.scroll[2] = UniformListScrollHandle::default();
        cx.notify();
        if !show
            || matches!(
                tab.changes.commit_files,
                Some(Load::Pending | Load::Ready(_))
            )
        {
            return;
        }
        let (Some(oid), Load::Ready(detail)) = (tab.history.selected.clone(), &tab.changes.detail)
        else {
            return;
        };
        let oid = crate::app::history::rebase::real_oid(&oid).to_owned();
        let changes = detail.changes.clone();
        let repo = tab.repo.clone();
        let request = tab.changes.request;
        tab.changes.commit_files = Some(Load::Pending);
        Self::spawn_background(
            window,
            cx,
            move || {
                let changed: std::collections::HashMap<_, _> = changes
                    .into_iter()
                    .map(|change| (change.path.clone(), change))
                    .collect();
                repo.committed_files(&oid).map(|files| {
                    files
                        .into_iter()
                        .map(|file| changed.get(&file.path).cloned().unwrap_or(file))
                        .collect()
                })
            },
            move |this, result, _, cx| {
                if let Some(changes) = this.changes_for_request(id, request) {
                    changes.commit_files = Some(result.into());
                    cx.notify();
                }
            },
        );
    }

    pub(super) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.branch_editor.is_editing() {
            self.submit_inline_branch(window, cx);
            return;
        }
        if self.modal_is_present() {
            return;
        }
        if let Some(id) = self
            .current()
            .filter(|tab| tab.rebase.is_some())
            .map(|tab| tab.id)
        {
            self.submit_rebase(id, window, cx);
            return;
        }
        if let Some(tab) = self.current().filter(|tab| tab.commit_enabled(cx)) {
            let edit = RepoOperation::Commit(
                tab.changes.summary.read(cx).value().to_string(),
                tab.changes.description.read(cx).value().to_string(),
            );
            self.update_repo(tab.id, Some(edit), window, cx);
        }
    }

    pub(super) fn right_panel(
        &self,
        tab: &RepoTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let panel = if tab.history.selected.is_none() {
            self.wip_panel(tab, cx)
        } else {
            self.commit_panel(tab, window, cx)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(p.border))
            .bg(rgb(p.surface_dim))
            .text_color(rgb(p.text))
            .child(panel)
    }
}

pub(super) fn change_count(tab: &RepoTab) -> usize {
    tab.history.data.worktree_summary.total()
}
