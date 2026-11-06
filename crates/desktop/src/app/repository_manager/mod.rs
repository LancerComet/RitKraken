mod view;

use std::{fs, path::PathBuf};

use gpui_kit::*;
use ritkraken_git::{self as git, Repo};

use super::{Client, history::History};

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

#[derive(Clone, PartialEq, Eq)]
pub(super) enum RepoList {
    Recent,
    Favorites,
    Project(PathBuf),
}

pub(super) struct RepositoryManager {
    pub(super) mode: Option<ManagerMode>,
    list: RepoList,
    projects: Vec<PathBuf>,
    pub(super) error: Option<String>,
    path: Entity<InputState>,
    url: Entity<InputState>,
    search: Entity<InputState>,
    _search: Subscription,
}

use gpui_kit::component::input::InputState;

impl RepositoryManager {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let path = cx.new(|cx| InputState::new(window, cx).placeholder("Repository directory"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("Repository URL"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter repos by name"));
        let subscription = cx.observe(&search, |_, _, cx| cx.notify());
        Self {
            mode: Some(ManagerMode::Open),
            list: RepoList::Recent,
            projects: Vec::new(),
            error: None,
            path,
            url,
            search,
            _search: subscription,
        }
    }
}

impl Client {
    pub(in crate::app) fn open_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.manager.mode = Some(ManagerMode::Open);
        self.preferences = None;
        self.manager.list = RepoList::Recent;
        self.branch_dialog.open = false;
        self.manager.error = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn browse(&mut self, project: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.opening {
            return;
        }
        let mode = self.manager.mode;
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
                Ok(Ok(Some(paths))) if mode == this.manager.mode => {
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
                            this.manager.path.update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().into_owned(), window, cx)
                            });
                        }
                    }
                }
                Ok(Err(error)) => {
                    this.manager.error = Some(error.to_string());
                    cx.notify();
                }
                _ => {}
            });
        })
        .detach();
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
                    let mut repositories: Vec<_> = fs::read_dir(root)?
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.path())
                        .filter(|path| path.join(".git").exists())
                        .collect();
                    repositories.sort();
                    Ok::<_, std::io::Error>(repositories)
                },
                move |this, result, _, cx| {
                    if this.manager.list == RepoList::Project(path) {
                        match result {
                            Ok(paths) => this.manager.projects = paths,
                            Err(error) => this.manager.error = Some(error.to_string()),
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
                self.manager.error = Some("Enter the repository location first.".into());
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
            move || {
                let repo = match request {
                    OpenRepo::Open(path) => Repo::open(&path),
                    OpenRepo::Clone(url, path) => Repo::clone(&url, &path),
                    OpenRepo::Init(path) => {
                        Repo::init(&path, (!branch.is_empty()).then_some(branch.as_str()))
                    }
                }?;
                let history = History::load(&repo, limit)?;
                Ok::<_, git::Error>((repo, history))
            },
            move |this, result, window, cx| {
                this.session.opening = false;
                match result {
                    Ok((repo, history)) => {
                        this.saved.remember(repo.root());
                        let id = this.insert_tab(repo, history, limit, window, cx);
                        this.show_tab(id, window, cx);
                    }
                    Err(error) => {
                        this.manager.mode = Some(this.manager.mode.unwrap_or(ManagerMode::Open));
                        this.manager.error = Some(error.to_string());
                    }
                }
                cx.notify();
            },
        );
        cx.notify();
    }
}
