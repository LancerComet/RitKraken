mod avatar;
mod changes;
mod export;
mod history;
mod message;
mod modals;
mod notices;
mod operations;
mod palette;
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
mod tabs;
mod theme;
mod toolbar;
mod watch;
mod widgets;
mod workspace;
mod worktrees;

use std::path::PathBuf;

use gpui_kit::*;

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
            rebase_modal: None,
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
