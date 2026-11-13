mod avatar;
mod changes;
mod export;
mod history;
mod message;
mod modals;
mod operations;
mod preferences;
mod refs;
mod remotes;
mod repository_manager;
mod review;
mod session;
mod settings;
mod shell;
mod system;
mod theme;
mod toolbar;
mod watch;
mod widgets;
mod workspace;

use std::path::PathBuf;

use gpui_kit::*;

use history::{
    BranchEditor,
    rebase::{
        RebaseDrop, RebaseEdit, RebaseFixup, RebaseMoveDown, RebaseMoveUp, RebasePick,
        RebaseReword, RebaseSquash,
    },
};
use message::Message;
use modals::{ClosingModal, ContentModal};
use operations::RepoOperation;
use preferences::PreferenceDraft;
use repository_manager::{OpenRepo, RepositoryManager};
use session::{RepoTab, Session, Tab};
use settings::SavedRepos;
use widgets::modal::ModalCompositor;
use workspace::WorkspacePanes;

actions!(
    ritkraken,
    [
        OpenRepos,
        OpenPreferences,
        Refresh,
        Escape,
        NextRow,
        PreviousRow,
        Submit
    ]
);

pub fn init(cx: &mut App) {
    theme::init(cx);
    widgets::tooltip::init(cx);
    bind_keys(cx);
}

fn bind_keys(cx: &mut App) {
    let (open, submit, preferences) = if cfg!(target_os = "macos") {
        ("cmd-o", "cmd-enter", "cmd-,")
    } else {
        ("ctrl-o", "ctrl-enter", "ctrl-,")
    };
    cx.bind_keys([
        KeyBinding::new(open, OpenRepos, Some("RitKraken")),
        KeyBinding::new(preferences, OpenPreferences, Some("RitKraken")),
        KeyBinding::new("f5", Refresh, Some("RitKraken")),
        KeyBinding::new("escape", Escape, Some("RitKraken")),
        KeyBinding::new("down", NextRow, Some("CommitGraph")),
        KeyBinding::new("up", PreviousRow, Some("CommitGraph")),
        KeyBinding::new("p", RebasePick, Some("CommitGraph")),
        KeyBinding::new("r", RebaseReword, Some("CommitGraph")),
        KeyBinding::new("e", RebaseEdit, Some("CommitGraph")),
        KeyBinding::new("s", RebaseSquash, Some("CommitGraph")),
        KeyBinding::new("f", RebaseFixup, Some("CommitGraph")),
        KeyBinding::new("d", RebaseDrop, Some("CommitGraph")),
        KeyBinding::new("alt-up", RebaseMoveUp, Some("CommitGraph")),
        KeyBinding::new("alt-down", RebaseMoveDown, Some("CommitGraph")),
        KeyBinding::new(submit, Submit, Some("RitKraken")),
        KeyBinding::new(submit, Submit, Some("CommitOperation")),
    ]);
}

enum Load<T> {
    Pending,
    Ready(T),
    Failed(Message),
}

impl<T, E: Into<Message>> From<Result<T, E>> for Load<T> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(value) => Self::Ready(value),
            Err(error) => Self::Failed(error.into()),
        }
    }
}

pub struct Client {
    session: Session,
    manager: RepositoryManager,
    saved: SavedRepos,
    preferences: Option<PreferenceDraft>,
    branch_editor: BranchEditor,
    closing_modal: Option<ClosingModal>,
    content_modal: Option<ContentModal>,
    modal_compositor: ModalCompositor,
    file_opener: system::FileOpener,
    panes: WorkspacePanes,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    _activation: Subscription,
    repo_watch: Option<watch::ActiveWatch>,
    auto_fetch_task: Option<Task<()>>,
}

impl Client {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = SavedRepos::load();
        saved.preferences.language.apply();
        theme::apply_theme(saved.preferences.theme, cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() && !this.modal_is_present() {
                if let Some(id) = this.session.active {
                    this.refresh_watched_repo(id, window, cx);
                }
            }
        });
        let mut app = Self {
            session: Session::default(),
            manager: RepositoryManager::new(window, cx),
            saved,
            preferences: None,
            branch_editor: BranchEditor::new(window, cx),
            closing_modal: None,
            content_modal: None,
            modal_compositor: ModalCompositor::default(),
            file_opener: system::FileOpener::default(),
            panes: WorkspacePanes::new(cx),
            focus,
            modal_focus: cx.focus_handle(),
            _activation: activation,
            repo_watch: None,
            auto_fetch_task: None,
        };
        app.reset_auto_fetch_timer(window, cx);
        if let Some(path) = path {
            app.open_repo(OpenRepo::Open(path), window, cx);
        } else if app.saved.preferences.remember_tabs && !app.saved.open_tabs.is_empty() {
            app.restore_tabs(window, cx);
        }
        app
    }

    fn current(&self) -> Option<&RepoTab> {
        self.session.active_tab()?.ready()
    }

    fn tab(&self, id: u64) -> Option<&RepoTab> {
        self.session.entry(id)?.ready()
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut RepoTab> {
        self.session.entry_mut(id)?.ready_mut()
    }

    fn spawn_background<T: Send + 'static, V: 'static>(
        window: &Window,
        cx: &Context<V>,
        work: impl FnOnce() -> T + Send + 'static,
        finish: impl FnOnce(&mut V, T, &mut Window, &mut Context<V>) + 'static,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = executor.spawn(async move { work() }).await;
            let _ = view.update_in(cx, |this, window, cx| finish(this, result, window, cx));
        })
        .detach();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.branch_editor.is_editing() {
            self.cancel_inline_branch(window, cx);
        } else if self.preferences.is_some() {
            self.save_preferences(window, cx);
        } else if self.has_open_modal() {
            if !self.session.opening {
                self.dismiss_modal(window, cx);
            }
        } else if self.closing_modal.is_none()
            && self
                .current()
                .is_some_and(|tab| tab.review.active.is_some())
        {
            self.close_diff(window, cx);
        } else if self.closing_modal.is_none()
            && let Some(id) = self
                .current()
                .filter(|tab| tab.rebase.is_some() && !tab.busy)
                .map(|tab| tab.id)
        {
            self.cancel_rebase(id, window, cx);
        }
        cx.notify();
    }
}
