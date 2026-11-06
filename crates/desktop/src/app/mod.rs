mod changes;
mod history;
mod message;
mod operations;
mod preferences;
mod refs;
mod repository_manager;
mod review;
mod session;
mod settings;
mod shell;
mod toolbar;
mod widgets;
mod workspace;

use std::path::PathBuf;

use gpui_kit::*;

use message::Message;
use operations::RepoOperation;
use preferences::PreferenceDraft;
use repository_manager::{OpenRepo, RepositoryManager};
use session::{RepoTab, Session};
use settings::SavedRepos;
use toolbar::BranchDialog;
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

pub fn bind_keys(cx: &mut App) {
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
        KeyBinding::new(submit, Submit, Some("RitKraken")),
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
    branch_dialog: BranchDialog,
    panes: WorkspacePanes,
    focus: FocusHandle,
    _activation: Subscription,
}

impl Client {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = SavedRepos::load();
        saved.preferences.language.apply();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active()
                && this.manager.mode.is_none()
                && this.preferences.is_none()
                && !this.branch_dialog.open
            {
                this.refresh(window, cx);
            }
        });
        let mut app = Self {
            session: Session::default(),
            manager: RepositoryManager::new(window, cx),
            saved,
            preferences: None,
            branch_dialog: BranchDialog::new(window, cx),
            panes: WorkspacePanes::new(cx),
            focus,
            _activation: activation,
        };
        if let Some(path) = path {
            app.open_repo(OpenRepo::Open(path), window, cx);
        } else if app.saved.preferences.remember_tabs && !app.saved.open_tabs.is_empty() {
            app.restore_tabs(window, cx);
        }
        app
    }

    fn current(&self) -> Option<&RepoTab> {
        self.tab(self.session.active?)
    }

    fn tab(&self, id: u64) -> Option<&RepoTab> {
        self.session.tabs.iter().find(|tab| tab.id == id)
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut RepoTab> {
        self.session.tabs.iter_mut().find(|tab| tab.id == id)
    }

    fn spawn_background<T: Send + 'static>(
        window: &Window,
        cx: &Context<Self>,
        work: impl FnOnce() -> T + Send + 'static,
        finish: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = executor.spawn(async move { work() }).await;
            let _ = view.update_in(cx, |this, window, cx| finish(this, result, window, cx));
        })
        .detach();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences.is_some() {
            self.preferences = None;
            self.focus.focus(window, cx);
        } else if self.branch_dialog.open {
            self.branch_dialog.open = false;
            self.focus.focus(window, cx);
        } else if self.manager.mode.is_some() && !self.session.opening {
            self.manager.mode = None;
            self.focus.focus(window, cx);
        } else if self
            .current()
            .is_some_and(|tab| tab.review.active.is_some())
        {
            self.close_diff(window, cx);
        }
        cx.notify();
    }
}
