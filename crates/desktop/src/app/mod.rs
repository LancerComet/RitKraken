mod avatar;
mod changes;
mod compare;
mod export;
mod history;
mod image;
mod layout;
mod logs;
mod message;
mod modals;
mod notices;
mod open_menu;
mod operations;
mod palette;
mod paths;
mod preferences;
mod pull;
mod push;
mod recovery;
mod refs;
mod remotes;
mod repository_manager;
mod review;
mod session;
mod settings;
mod shell;
mod shortcuts;
mod system;
mod theme;
mod titlebar;
mod toolbar;
mod watch;
mod widgets;
mod workspace;
mod worktrees;

use std::path::PathBuf;

use gpui_kit::*;

pub use layout::{MIN_WINDOW_SIZE, saved_window_bounds};

use history::BranchEditor;
use message::Message;
use modals::{ClosingModal, ContentModal};
use notices::Notice;
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
        OpenPalette,
        SearchCommits,
        Refresh,
        Escape,
        NextRow,
        PreviousRow,
        Submit
    ]
);

pub fn init(cx: &mut App) {
    layout::init(cx);
    theme::init(cx);
    widgets::tooltip::init(cx);
    shortcuts::init(cx);
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
    graph_filters: history::filter::Store,
    preferences: Option<PreferenceDraft>,
    branch_editor: BranchEditor,
    closing_modal: Option<ClosingModal>,
    content_modal: Option<ContentModal>,
    rebase_modal: Option<Box<history::rebase::RebaseDialog>>,
    modal_compositor: ModalCompositor,
    file_opener: system::FileOpener,
    panes: WorkspacePanes,
    focus: FocusHandle,
    modal_focus: FocusHandle,
    _activation: Subscription,
    _appearance: Subscription,
    repo_watch: Option<watch::ActiveWatch>,
    auto_fetch_task: Option<Task<()>>,
}

impl Client {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = SavedRepos::load();
        logs::init(saved.preferences.log_file_size_mb);
        saved.preferences.language.apply();
        theme::apply_theme(
            theme::resolve_theme(saved.preferences.theme, window.appearance()),
            cx,
        );
        layout::remember_window(window, cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() && !this.modal_is_present() {
                if let Some(id) = this.session.active {
                    this.refresh_watched_repo(id, window, cx);
                }
            }
        });
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            if this.saved.preferences.theme.is_none() {
                theme::apply_theme(theme::resolve_theme(None, window.appearance()), cx);
                cx.notify();
            }
        });
        let graph_filters = history::filter::load();
        let mut app = Self {
            session: Session::default(),
            manager: RepositoryManager::new(window, cx),
            saved,
            graph_filters,
            preferences: None,
            branch_editor: BranchEditor::new(window, cx),
            closing_modal: None,
            content_modal: None,
            rebase_modal: None,
            modal_compositor: ModalCompositor::default(),
            file_opener: system::FileOpener::default(),
            panes: WorkspacePanes::new(cx),
            focus,
            modal_focus: cx.focus_handle(),
            _activation: activation,
            _appearance: appearance,
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
        if let Some(dialog) = &self.rebase_modal
            && dialog.tab.id == id
        {
            return Some(&dialog.tab);
        }
        self.session.entry(id)?.ready()
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut RepoTab> {
        if let Some(dialog) = &mut self.rebase_modal
            && dialog.tab.id == id
        {
            return Some(&mut dialog.tab);
        }
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
