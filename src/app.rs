mod view;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::{
    component::{
        diff::{DiffFile, DiffMode, DiffState},
        input::{InputEvent, InputState, TextareaState},
    },
    *,
};

use crate::{
    git::{self, Change, CommitDetail, FileTarget, PATCH_VIEW_LIMIT, Patch, RefKind, Repo, Scope},
    graph::History,
};

actions!(
    ritkraken,
    [OpenRepos, Refresh, Escape, NextRow, PreviousRow, Submit]
);

pub fn bind_keys(cx: &mut App) {
    let (open, submit) = if cfg!(target_os = "macos") {
        ("cmd-o", "cmd-enter")
    } else {
        ("ctrl-o", "ctrl-enter")
    };
    cx.bind_keys([
        KeyBinding::new(open, OpenRepos, Some("RitKraken")),
        KeyBinding::new("f5", Refresh, Some("RitKraken")),
        KeyBinding::new("escape", Escape, Some("RitKraken")),
        KeyBinding::new("down", NextRow, Some("CommitGraph")),
        KeyBinding::new("up", PreviousRow, Some("CommitGraph")),
        KeyBinding::new(submit, Submit, Some("RitKraken")),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManagerMode {
    Open,
    Clone,
    Init,
}

enum OpenRepo {
    Open(PathBuf),
    Clone(String, PathBuf),
    Init(PathBuf),
}

#[derive(Clone, PartialEq, Eq)]
enum RepoList {
    Recent,
    Favorites,
    Project(PathBuf),
}

#[derive(Default)]
struct SavedRepos {
    recent: Vec<PathBuf>,
    favorites: Vec<PathBuf>,
    projects: Vec<PathBuf>,
}

impl SavedRepos {
    fn load() -> Self {
        let Some(value) = settings_path()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        else {
            return Self::default();
        };
        let paths = |key: &str| serde_json::from_value(value[key].clone()).unwrap_or_default();
        Self {
            recent: paths("recent"),
            favorites: paths("favorites"),
            projects: paths("projects"),
        }
    }

    fn save(&self) {
        if let Some(path) = settings_path() {
            let json = serde_json::json!({
                "recent": self.recent,
                "favorites": self.favorites,
                "projects": self.projects,
            });
            if let Some(parent) = path.parent() {
                if fs::create_dir_all(parent).is_ok() {
                    let temporary = path.with_extension(format!("{}.new", std::process::id()));
                    if fs::write(&temporary, json.to_string()).is_ok() {
                        let _ = fs::rename(temporary, path);
                    }
                }
            }
        }
    }

    fn remember(&mut self, root: &Path) {
        self.recent.retain(|path| path != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(25);
        self.save();
    }
}

fn settings_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RITKRAKEN_STATE_DIR") {
        return Some(PathBuf::from(path).join("repositories.json"));
    }
    #[cfg(windows)]
    let directory = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("RitKraken");
    #[cfg(target_os = "macos")]
    let directory =
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/RitKraken");
    #[cfg(not(any(windows, target_os = "macos")))]
    let directory = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?
        .join("ritkraken");
    Some(directory.join("repositories.json"))
}

enum Load<T> {
    Pending,
    Ready(T),
    Failed(String),
}

struct DiffView {
    file: FileTarget,
    patch: Load<Arc<Patch>>,
    viewer: Option<Entity<DiffState>>,
    message: Option<String>,
    hunks: Vec<String>,
}

struct RepoTab {
    id: u64,
    repo: Repo,
    history: Arc<History>,
    selected: Option<String>,
    detail: Load<CommitDetail>,
    diff: Option<DiffView>,
    request: u64,
    busy: bool,
    notice: Option<(bool, String)>,
    scroll: UniformListScrollHandle,
    graph_focus: FocusHandle,
    filter: Entity<InputState>,
    summary: Entity<InputState>,
    description: Entity<TextareaState>,
    closed_groups: HashSet<RefKind>,
    closed_folders: HashSet<String>,
    file_tree: bool,
    diff_mode: DiffMode,
    _subscriptions: Vec<Subscription>,
}

impl RepoTab {
    fn initial_selection(history: &History) -> Option<String> {
        let status = &history.snapshot.status;
        if status.staged.is_empty() && status.unstaged.is_empty() {
            history.snapshot.head.clone()
        } else {
            None
        }
    }

    fn has_changes(&self) -> bool {
        let status = &self.history.snapshot.status;
        !status.unstaged.is_empty() || !status.staged.is_empty()
    }

    fn show_wip(&self) -> bool {
        self.has_changes() || self.selected.is_none()
    }

    fn commit_enabled(&self, cx: &App) -> bool {
        self.selected.is_none()
            && !self.busy
            && !self.history.snapshot.status.staged.is_empty()
            && !self.summary.read(cx).value().trim().is_empty()
    }
}

enum Edit {
    Stage(Option<Change>),
    Unstage(Option<Change>),
    Hunk(FileTarget, Arc<Patch>, usize),
    Commit(String, String),
    Switch(String),
    Branch(String),
}

impl Edit {
    fn execute(&self, repo: &Repo) -> git::Result<()> {
        match self {
            Self::Stage(change) => repo.stage(change.as_ref()),
            Self::Unstage(change) => repo.unstage(change.as_ref()),
            Self::Hunk(file, patch, index) => repo.apply_hunk(file, patch, *index),
            Self::Commit(summary, description) => repo.commit(summary, description),
            Self::Switch(branch) => repo.switch(branch),
            Self::Branch(branch) => repo.create_branch(branch),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Stage(_) => "Changes staged",
            Self::Unstage(_) => "Changes unstaged",
            Self::Hunk(file, ..) if file.scope == Scope::Index => "Hunk unstaged",
            Self::Hunk(..) => "Hunk staged",
            Self::Commit(..) => "Commit created",
            Self::Switch(_) => "Branch checked out",
            Self::Branch(_) => "Branch created",
        }
    }
}

pub struct Client {
    tabs: Vec<RepoTab>,
    active: Option<u64>,
    next_tab: u64,
    manager: Option<ManagerMode>,
    repo_list: RepoList,
    project_repos: Vec<PathBuf>,
    opening: bool,
    manager_error: Option<String>,
    saved: SavedRepos,
    path: Entity<InputState>,
    url: Entity<InputState>,
    search: Entity<InputState>,
    branch_name: Entity<InputState>,
    branch_dialog: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Client {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repo_path =
            cx.new(|cx| InputState::new(window, cx).placeholder("Repository directory"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("Repository URL"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter repos by name"));
        let branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("Branch name"));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let subscriptions = vec![
            cx.observe(&search, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() && this.manager.is_none() && !this.branch_dialog {
                    this.refresh(window, cx);
                }
            }),
        ];
        let mut this = Self {
            tabs: Vec::new(),
            active: None,
            next_tab: 0,
            manager: Some(ManagerMode::Open),
            repo_list: RepoList::Recent,
            project_repos: Vec::new(),
            opening: false,
            manager_error: None,
            saved: SavedRepos::load(),
            path: repo_path,
            url,
            search,
            branch_name,
            branch_dialog: false,
            focus,
            _subscriptions: subscriptions,
        };
        if let Some(path) = path {
            this.open_repo(OpenRepo::Open(path), window, cx);
        }
        this
    }

    fn current(&self) -> Option<&RepoTab> {
        self.tabs.iter().find(|tab| Some(tab.id) == self.active)
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut RepoTab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    fn open_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.manager = Some(ManagerMode::Open);
        self.repo_list = RepoList::Recent;
        self.branch_dialog = false;
        self.manager_error = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn show_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.active = Some(id);
            self.manager = None;
            self.branch_dialog = false;
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id && !tab.busy) else {
            return;
        };
        self.tabs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.id);
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn browse(&mut self, project: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening {
            return;
        }
        let mode = self.manager;
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                if project {
                    "Choose project directory"
                } else {
                    "Choose repository directory"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = prompt.await;
            let _ = view.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) if mode == this.manager => {
                    if let Some(path) = paths.into_iter().next() {
                        if project {
                            if !this.saved.projects.contains(&path) {
                                this.saved.projects.push(path.clone());
                                this.saved.save();
                            }
                            this.change_repo_list(RepoList::Project(path), window, cx);
                        } else if mode == Some(ManagerMode::Open) {
                            this.open_repo(OpenRepo::Open(path), window, cx);
                        } else {
                            this.path.update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().into_owned(), window, cx)
                            });
                        }
                    }
                }
                Ok(Err(error)) => {
                    this.manager_error = Some(error.to_string());
                    cx.notify();
                }
                _ => {}
            });
        })
        .detach();
    }

    fn change_repo_list(&mut self, list: RepoList, window: &mut Window, cx: &mut Context<Self>) {
        self.repo_list = list.clone();
        self.project_repos.clear();
        self.manager_error = None;
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        if let RepoList::Project(path) = list {
            let executor = cx.background_executor().clone();
            cx.spawn_in(window, async move |view, cx| {
                let root = path.clone();
                let result = executor
                    .spawn(async move {
                        let mut repositories: Vec<_> = fs::read_dir(root)?
                            .filter_map(|entry| entry.ok())
                            .map(|entry| entry.path())
                            .filter(|path| path.join(".git").exists())
                            .collect();
                        repositories.sort();
                        Ok::<_, std::io::Error>(repositories)
                    })
                    .await;
                let _ = view.update_in(cx, |this, _, cx| {
                    if this.repo_list == RepoList::Project(path) {
                        match result {
                            Ok(paths) => this.project_repos = paths,
                            Err(error) => this.manager_error = Some(error.to_string()),
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn toggle_favorite(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.saved.favorites.contains(&path) {
            self.saved.favorites.retain(|favorite| favorite != &path);
        } else {
            self.saved.favorites.push(path);
        }
        self.saved.save();
        cx.notify();
    }

    fn open_from_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path.read(cx).value();
        let url = self.url.read(cx).value();
        let path = path.trim();
        let url = url.trim();
        let request = match self.manager {
            Some(ManagerMode::Open) if !path.is_empty() => OpenRepo::Open(path.into()),
            Some(ManagerMode::Init) if !path.is_empty() => OpenRepo::Init(path.into()),
            Some(ManagerMode::Clone) if !path.is_empty() && !url.is_empty() => {
                OpenRepo::Clone(url.into(), path.into())
            }
            _ => {
                self.manager_error = Some("Enter the repository location first.".into());
                cx.notify();
                return;
            }
        };
        self.open_repo(request, window, cx);
    }

    fn open_repo(&mut self, request: OpenRepo, window: &mut Window, cx: &mut Context<Self>) {
        if self.opening {
            return;
        }
        self.opening = true;
        self.manager_error = None;
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = executor
                .spawn(async move {
                    let repo = match request {
                        OpenRepo::Open(path) => Repo::open(&path),
                        OpenRepo::Clone(url, path) => Repo::clone(&url, &path),
                        OpenRepo::Init(path) => Repo::init(&path),
                    }?;
                    let history = Arc::new(History::new(repo.snapshot()?));
                    Ok::<_, String>((repo, history))
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.opening = false;
                match result {
                    Ok((repo, history)) => this.add_tab(repo, history, window, cx),
                    Err(error) => {
                        this.manager = Some(this.manager.unwrap_or(ManagerMode::Open));
                        this.manager_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn add_tab(
        &mut self,
        repo: Repo,
        history: Arc<History>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.saved.remember(&repo.root);
        if let Some(id) = self
            .tabs
            .iter()
            .find(|tab| tab.repo.root == repo.root)
            .map(|tab| tab.id)
        {
            self.show_tab(id, window, cx);
            return;
        }
        let id = self.next_tab;
        self.next_tab += 1;
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter branches and tags"));
        let summary = cx.new(|cx| InputState::new(window, cx).placeholder("Summary"));
        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("Description")
        });
        let subscriptions = vec![
            cx.observe(&filter, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &summary,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter {
                        secondary: true, ..
                    } => this.submit(window, cx),
                    _ => {}
                },
            ),
        ];
        let selection = RepoTab::initial_selection(&history);
        self.tabs.push(RepoTab {
            id,
            repo,
            history,
            selected: None,
            detail: Load::Pending,
            diff: None,
            request: 0,
            busy: false,
            notice: None,
            scroll: UniformListScrollHandle::default(),
            graph_focus: cx.focus_handle(),
            filter,
            summary,
            description,
            closed_groups: HashSet::new(),
            closed_folders: HashSet::new(),
            file_tree: false,
            diff_mode: DiffMode::Split,
            _subscriptions: subscriptions,
        });
        self.show_tab(id, window, cx);
        self.select(id, selection, Some(ScrollStrategy::Top), window, cx);
    }

    fn select(
        &mut self,
        id: u64,
        selected: Option<String>,
        scroll: Option<ScrollStrategy>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.selected = selected;
        tab.diff = None;
        tab.detail = Load::Pending;
        tab.request += 1;
        let request = tab.request;
        let index = tab
            .selected
            .as_ref()
            .and_then(|oid| tab.history.position.get(oid).copied());
        if let Some(scroll) = scroll {
            tab.scroll.scroll_to_item(
                index.map_or(0, |index| index + usize::from(tab.show_wip())),
                scroll,
            );
        }
        if let Some(index) = index {
            let commit = tab.history.snapshot.commits[index].clone();
            let repo = tab.repo.clone();
            let executor = cx.background_executor().clone();
            cx.spawn_in(window, async move |view, cx| {
                let result = executor.spawn(async move { repo.detail(&commit) }).await;
                let _ = view.update_in(cx, |this, _, cx| {
                    if let Some(tab) = this.tab_mut(id).filter(|tab| tab.request == request) {
                        tab.detail = match result {
                            Ok(detail) => Load::Ready(detail),
                            Err(error) => Load::Failed(error),
                        };
                        cx.notify();
                    }
                });
            })
            .detach();
        } else if tab.selected.is_some() {
            tab.detail = Load::Failed("This ref is outside the loaded history.".into());
        }
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.current() else { return };
        let offset = usize::from(tab.show_wip());
        let count = tab.history.snapshot.commits.len() + offset;
        let position = tab
            .selected
            .as_ref()
            .and_then(|oid| tab.history.position.get(oid))
            .map_or(0, |index| *index + offset);
        let index = position
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        let selected = index
            .checked_sub(offset)
            .and_then(|index| tab.history.snapshot.commits.get(index))
            .map(|commit| commit.id.clone());
        self.select(tab.id, selected, Some(ScrollStrategy::Nearest), window, cx);
    }

    fn open_diff(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.request += 1;
        let request = tab.request;
        let repo = tab.repo.clone();
        let target = file.clone();
        tab.diff = Some(DiffView {
            file,
            patch: Load::Pending,
            viewer: None,
            message: None,
            hunks: Vec::new(),
        });
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let result = executor
                .spawn(async move {
                    let patch = Arc::new(repo.patch(&target)?);
                    let hunks = patch.hunk_labels();
                    let viewer = if patch.bytes().is_empty() {
                        Err("No textual changes for this file.".into())
                    } else if patch.bytes().len() > PATCH_VIEW_LIMIT {
                        Err("This diff exceeds the 8 MiB preview limit.".into())
                    } else {
                        match DiffFile::parse(&String::from_utf8_lossy(patch.bytes())) {
                            Ok(files) if !files.is_empty() => Ok(files),
                            Ok(_) => Err("No textual changes for this file.".into()),
                            Err(error) => Err(format!(
                                "Git produced a diff this viewer cannot display.\n{error}"
                            )),
                        }
                    };
                    Ok::<_, String>((patch, hunks, viewer))
                })
                .await;
            let _ = view.update_in(cx, |this, _, cx| {
                let Some(tab) = this.tab_mut(id).filter(|tab| tab.request == request) else {
                    return;
                };
                let Some(diff) = tab.diff.as_mut() else {
                    return;
                };
                match result {
                    Ok((patch, hunks, viewer)) => {
                        diff.patch = Load::Ready(patch);
                        diff.hunks = hunks;
                        match viewer {
                            Ok(files) => {
                                diff.viewer = Some(cx.new(|cx| {
                                    let mut state = DiffState::new(files, cx);
                                    state.set_mode(tab.diff_mode, cx);
                                    state
                                }));
                            }
                            Err(message) => diff.message = Some(message),
                        }
                    }
                    Err(error) => diff.patch = Load::Failed(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn set_diff_mode(&mut self, id: u64, mode: DiffMode, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.diff_mode = mode;
        if let Some(viewer) = tab.diff.as_ref().and_then(|diff| diff.viewer.as_ref()) {
            viewer.update(cx, |state, cx| state.set_mode(mode, cx));
        }
        cx.notify();
    }

    fn close_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            if let Some(tab) = self.tab_mut(id) {
                tab.diff = None;
                tab.request += 1;
                tab.graph_focus.focus(window, cx);
                cx.notify();
            }
        }
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            self.update_repo(id, None, window, cx);
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.manager.is_some() || self.branch_dialog {
            return;
        }
        if let Some(tab) = self.current().filter(|tab| tab.commit_enabled(cx)) {
            let edit = Edit::Commit(
                tab.summary.read(cx).value().to_string(),
                tab.description.read(cx).value().to_string(),
            );
            self.update_repo(tab.id, Some(edit), window, cx);
        }
    }

    fn update_repo(
        &mut self,
        id: u64,
        edit: Option<Edit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        tab.notice = None;
        let repo = tab.repo.clone();
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, cx| {
            let (result, history) = executor
                .spawn(async move {
                    let result = edit.as_ref().map(|edit| edit.execute(&repo));
                    let history = repo.snapshot().map(History::new).map(Arc::new);
                    let result = result.zip(edit);
                    (result, history)
                })
                .await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.finish_update(id, result, history, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_update(
        &mut self,
        id: u64,
        result: Option<(git::Result<()>, Edit)>,
        history: git::Result<Arc<History>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.busy = false;
        let mut committed = false;
        let mut switched = false;
        if let Some((result, edit)) = result {
            match result {
                Ok(()) => {
                    tab.notice = Some((false, edit.label().into()));
                    committed = matches!(edit, Edit::Commit(..));
                    switched = matches!(edit, Edit::Switch(_) | Edit::Branch(_));
                    if committed {
                        tab.summary
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        tab.description
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
                Err(error) => tab.notice = Some((true, error)),
            }
        }
        match history {
            Ok(history) => tab.history = history,
            Err(error) => {
                let message = tab.notice.as_ref().map_or(error.clone(), |(_, previous)| {
                    format!("{previous}\nRefresh failed: {error}")
                });
                tab.notice = Some((true, message));
                cx.notify();
                return;
            }
        }
        if committed || switched {
            let selected = if committed {
                tab.history.snapshot.head.clone()
            } else {
                RepoTab::initial_selection(&tab.history)
            };
            self.select(id, selected, Some(ScrollStrategy::Top), window, cx);
        } else if let Some(file) = tab.diff.as_ref().map(|diff| diff.file.clone()) {
            let updated = match file.scope {
                Scope::Worktree | Scope::Index => {
                    let status = &tab.history.snapshot.status;
                    let candidates = if file.scope == Scope::Index {
                        [
                            (&status.staged, Scope::Index),
                            (&status.unstaged, Scope::Worktree),
                        ]
                    } else {
                        [
                            (&status.unstaged, Scope::Worktree),
                            (&status.staged, Scope::Index),
                        ]
                    };
                    candidates.into_iter().find_map(|(changes, scope)| {
                        changes
                            .iter()
                            .find(|change| change.path == file.change.path)
                            .cloned()
                            .map(|change| FileTarget { scope, change })
                    })
                }
                Scope::Commit { .. } => Some(file),
            };
            if let Some(file) = updated {
                self.open_diff(id, file, window, cx);
            } else {
                tab.diff = None;
                tab.request += 1;
            }
        } else if tab
            .selected
            .as_ref()
            .is_some_and(|oid| !tab.history.position.contains_key(oid))
        {
            let selected = RepoTab::initial_selection(&tab.history);
            self.select(id, selected, None, window, cx);
        }
        cx.notify();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.branch_dialog {
            self.branch_dialog = false;
            self.focus.focus(window, cx);
        } else if self.manager.is_some() && !self.opening {
            self.manager = None;
            self.focus.focus(window, cx);
        } else if self.current().is_some_and(|tab| tab.diff.is_some()) {
            self.close_diff(window, cx);
        }
        cx.notify();
    }
}
