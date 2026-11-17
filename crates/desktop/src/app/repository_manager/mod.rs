mod list;
mod view;

use std::{fs, path::PathBuf, sync::Arc};

use gpui_kit::*;
use ritkraken_git::{self as git, Repo};

use crate::i18n::LocalizedInput;

use self::list::NameOrder;
use super::{Client, Message, history::History, system::choose_directory};

const ICON_SIZE: Pixels = px(12. * 1.2);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ManagerMode {
    Open,
    Clone,
    Init,
}

pub(super) enum OpenRepo {
    Open(PathBuf),
    Clone(String, PathBuf),
    Init(PathBuf),
}

impl OpenRepo {
    pub(super) fn load(
        self,
        limit: Option<usize>,
        default_branch: Option<&str>,
    ) -> git::Result<(Repo, Arc<History>)> {
        let repo = match self {
            Self::Open(path) => Repo::open(&path),
            Self::Clone(url, path) => Repo::clone(&url, &path),
            Self::Init(path) => Repo::init(&path, default_branch),
        }?;
        let history = History::load(&repo, limit)?;
        Ok((repo, history))
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
                OpenRepo::Clone(url.into(), path.into())
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
        Self::spawn_background(
            window,
            cx,
            move || request.load(limit, (!branch.is_empty()).then_some(branch.as_str())),
            move |this, result, window, cx| {
                this.session.opening = false;
                match result {
                    Ok((repo, history)) => {
                        this.saved.remember(repo.root());
                        let id = this.insert_tab(repo, history, limit, None, window, cx);
                        this.show_tab(id, window, cx);
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
}
