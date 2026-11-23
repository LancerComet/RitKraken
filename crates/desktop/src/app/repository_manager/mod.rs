mod list;
mod view;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{prelude::*, *};
use ritkraken_git::{self as git, Repo};

use crate::i18n::{LocalizedInput, tr};

use self::list::NameOrder;
use super::{
    Client, Message, Notice, history::History, session::Tab, system::choose_directory,
    theme::Palette, widgets::icon,
};

const ICON_SIZE: Pixels = px(12. * 1.2);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ManagerMode {
    Open,
    Clone,
    Init,
}

pub(super) enum OpenRepo {
    Open(PathBuf),
    Clone(String, PathBuf, bool),
    Init(PathBuf),
}

impl OpenRepo {
    pub(super) fn load(
        self,
        limit: Option<usize>,
        default_branch: Option<&str>,
        filters: &super::history::filter::Store,
    ) -> git::Result<(Repo, Arc<History>, Option<git::Error>)> {
        let (repo, submodules_error) = match self {
            Self::Open(path) => (Repo::open(&path)?, None),
            Self::Init(path) => (Repo::init(&path, default_branch)?, None),
            Self::Clone(url, path, submodules) => {
                let repo = Repo::clone(&url, &path)?;
                let error = if submodules {
                    repo.init_submodules_recursive().err()
                } else {
                    None
                };
                (repo, error)
            }
        };
        let filter = filters
            .get(&super::history::filter::root_key(&repo))
            .cloned()
            .unwrap_or_default();
        let history = History::load(&repo, limit, &filter)?;
        Ok((repo, history, submodules_error))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum RepoList {
    Recent,
    Favorites,
    Project(PathBuf),
}

pub(super) struct RepositoryManager {
    pub(super) mode: Option<ManagerMode>,
    list: RepoList,
    name_order: Option<NameOrder>,
    projects: Vec<PathBuf>,
    pub(super) error: Option<Message>,
    pub(super) clone_submodules: bool,
    path: LocalizedInput,
    url: LocalizedInput,
    search: LocalizedInput,
    _search: Subscription,
}

use gpui_kit::component::input::InputState;

impl RepositoryManager {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let path = LocalizedInput::new("app.repositories.directory", cx, |cx| {
            InputState::new(window, cx)
        });
        let url = LocalizedInput::new("app.repositories.url", cx, |cx| InputState::new(window, cx));
        let search = LocalizedInput::new("app.repositories.filter", cx, |cx| {
            InputState::new(window, cx)
        });
        let subscription = cx.observe(&search, |_, _, cx| cx.notify());
        Self {
            mode: Some(ManagerMode::Open),
            list: RepoList::Recent,
            name_order: None,
            projects: Vec::new(),
            error: None,
            clone_submodules: true,
            path,
            url,
            search,
            _search: subscription,
        }
    }

    pub(super) fn localize(&self, window: &mut Window, cx: &mut App) {
        for input in [&self.path, &self.url, &self.search] {
            input.localize(window, cx);
        }
    }
}

impl Client {
    pub(in crate::app) fn open_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.content_modal.is_some() || self.rebase_modal.is_some() {
            return;
        }
        if self.preferences.is_some() {
            self.save_preferences(window, cx);
            if self.preferences.is_some() {
                return;
            }
        }
        self.manager.mode = Some(ManagerMode::Open);
        self.manager.list = RepoList::Recent;
        self.branch_editor.clear();
        self.manager.error = None;
        self.modal_focus.focus(window, cx);
        cx.notify();
    }

    fn browse(&mut self, project: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.opening {
            return;
        }
        let mode = self.manager.mode;
        let prompt = if project {
            "app.repositories.choose_project"
        } else {
            "app.repositories.choose_repository"
        };
        choose_directory(
            prompt,
            window,
            cx,
            move |this, result, window, cx| match result {
                Ok(path) if mode == this.manager.mode => {
                    if project {
                        if !this.saved.projects.contains(&path) {
                            this.saved.projects.push(path.clone());
                            this.saved.save();
                        }
                        this.change_repo_list(RepoList::Project(path), window, cx);
                    } else if mode == Some(ManagerMode::Open) {
                        this.open_repo(OpenRepo::Open(path), window, cx);
                    } else {
                        this.manager.path.update(cx, |input, cx| {
                            input.set_value(path.to_string_lossy().into_owned(), window, cx)
                        });
                    }
                }
                Ok(_) => {}
                Err(message) => {
                    this.manager.error = Some(message);
                    cx.notify();
                }
            },
        );
    }

    fn change_repo_list(&mut self, list: RepoList, window: &mut Window, cx: &mut Context<Self>) {
        self.manager.list = list.clone();
        self.manager.projects.clear();
        self.manager.error = None;
        self.manager
            .search
            .update(cx, |search, cx| search.set_value("", window, cx));
        if let RepoList::Project(path) = list {
            let root = path.clone();
            Self::spawn_background(
                window,
                cx,
                move || {
                    let repositories = fs::read_dir(root)?
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.path())
                        .filter(|path| path.join(".git").exists())
                        .collect();
                    Ok::<_, std::io::Error>(repositories)
                },
                move |this, result, _, cx| {
                    if this.manager.list == RepoList::Project(path) {
                        match result {
                            Ok(paths) => this.manager.projects = paths,
                            Err(error) => {
                                this.manager.error =
                                    Some(Message::detail("app.error.list_repositories", error))
                            }
                        }
                        cx.notify();
                    }
                },
            );
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
        let path = self.manager.path.read(cx).value();
        let url = self.manager.url.read(cx).value();
        let path = path.trim();
        let url = url.trim();
        let request = match self.manager.mode {
            Some(ManagerMode::Open) if !path.is_empty() => OpenRepo::Open(path.into()),
            Some(ManagerMode::Init) if !path.is_empty() => OpenRepo::Init(path.into()),
            Some(ManagerMode::Clone) if !path.is_empty() && !url.is_empty() => {
                OpenRepo::Clone(url.into(), path.into(), self.manager.clone_submodules)
            }
            _ => {
                self.manager.error = Some(Message::Key("app.error.repository_location"));
                cx.notify();
                return;
            }
        };
        self.open_repo(request, window, cx);
    }

    pub(in crate::app) fn open_repo(
        &mut self,
        request: OpenRepo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.opening {
            return;
        }
        self.session.opening = true;
        self.manager.error = None;
        let limit = self.saved.preferences.history_limit();
        let branch = self.saved.preferences.default_branch.clone();
        let filters = self.graph_filters.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                request.load(
                    limit,
                    (!branch.is_empty()).then_some(branch.as_str()),
                    &filters,
                )
            },
            move |this, result, window, cx| {
                this.session.opening = false;
                match result {
                    Ok((repo, history, submodules_error)) => {
                        this.saved.remember(repo.root());
                        let id = this.insert_tab(repo, history, limit, None, window, cx);
                        this.show_tab(id, window, cx);
                        if let Some(error) = submodules_error
                            && let Some(tab) = this.tab_mut(id)
                        {
                            tab.notify(
                                Notice::warning(Message::detail(
                                    "app.notice.clone_submodules_failed",
                                    error,
                                )),
                                window,
                                cx,
                            );
                        }
                    }
                    Err(error) => {
                        this.manager.mode = Some(this.manager.mode.unwrap_or(ManagerMode::Open));
                        this.manager.error = Some(error.into());
                    }
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(in crate::app) fn open_recent_repo(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.opening {
            return;
        }
        if let Some(id) = self
            .session
            .tabs
            .iter()
            .find(|tab| tab.root() == path.as_path())
            .map(Tab::id)
        {
            self.saved.remember(&path);
            self.show_tab(id, window, cx);
            return;
        }
        self.open_repo(OpenRepo::Open(path), window, cx);
    }

    pub(in crate::app) fn remove_recent_repo(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let removed = self.saved.remove_recent(path);
        let close_id = self
            .session
            .tabs
            .iter()
            .find(|tab| {
                tab.root() == path && self.session.active != Some(tab.id()) && tab.can_close()
            })
            .map(Tab::id);
        if let Some(id) = close_id {
            self.close_tab(id, window, cx);
        } else if removed {
            self.saved.save();
            cx.notify();
        }
    }
}

pub(in crate::app) fn remove_recent_button(
    id: impl Into<ElementId>,
    group: SharedString,
    p: Palette,
    on_remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label: SharedString = tr!("app.repositories.remove_recent").into();
    div()
        .id(id)
        .group(group.clone())
        .size(px(24.))
        .flex_shrink_0()
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(p.danger_bg)))
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .child(
            icon("icons/x.svg", p.text_subtle)
                .size(px(12.))
                .group_hover(group, move |s| s.text_color(rgb(p.danger))),
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_remove(event, window, cx);
        })
}
