use std::path::PathBuf;

use gpui_kit::{
    component::{
        Disableable,
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
    },
    prelude::*,
    *,
};
use ritkraken_git::{NewWorktree, Repo, Worktree};

use super::{
    Client, Escape, Load, Message, RepoOperation,
    modals::{ContentModal, ModalHandle},
    repository_manager::OpenRepo,
    theme::palette,
    widgets::{Tone, button, error_text},
};
use crate::i18n::tr;

struct WorktreeForm {
    repo_id: u64,
    repo: Repo,
    client: WeakEntity<Client>,
    modal: ModalHandle,
    entries: Load<Vec<Worktree>>,
    opened: Vec<PathBuf>,
    path: Entity<InputState>,
    branch: Entity<InputState>,
    create_branch: bool,
    generation: u64,
    error: Option<Message>,
    _inputs: [Subscription; 2],
}

impl Client {
    pub(super) fn open_worktrees(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_is_present() {
            return;
        }
        let Some(tab) = self.current().filter(|tab| !tab.busy) else {
            return;
        };
        let id = tab.id;
        let repo = tab.repo.clone();
        let opened = self
            .session
            .tabs
            .iter()
            .map(|tab| tab.root().to_path_buf())
            .collect();
        let client = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let path =
                cx.new(|cx| InputState::new(window, cx).placeholder(tr!("app.worktrees.path")));
            let branch =
                cx.new(|cx| InputState::new(window, cx).placeholder(tr!("app.worktrees.branch")));
            let inputs = [&path, &branch].map(|input| {
                cx.subscribe(input, |this: &mut WorktreeForm, _, _: &InputEvent, cx| {
                    this.error = None;
                    cx.notify();
                })
            });
            WorktreeForm {
                repo_id: id,
                repo,
                client,
                modal: handle,
                entries: Load::Pending,
                opened,
                path,
                branch,
                create_branch: true,
                generation: 0,
                error: None,
                _inputs: inputs,
            }
        });
        view.update(cx, |form, cx| form.reload(window, cx));
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.worktrees.title"),
                view,
                size(px(900.), px(680.)),
                false,
            ),
            window,
            cx,
        );
    }
}

impl WorktreeForm {
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.repo.clone();
        self.generation += 1;
        let generation = self.generation;
        self.entries = Load::Pending;
        Client::spawn_background(
            window,
            cx,
            move || repo.worktrees(),
            move |this, result, _, cx| {
                if this.generation == generation {
                    this.entries = result.into();
                    cx.notify();
                }
            },
        );
    }

    fn run(&mut self, operation: RepoOperation, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_busy() || !self.modal.is_open() {
            return;
        }
        let id = self.repo_id;
        let form = cx.weak_entity();
        self.modal.set_busy(true);
        self.error = None;
        let result = self.client.update(cx, |client, cx| {
            client.update_repo_with_completion(
                id,
                operation,
                move |client, succeeded, window, cx| {
                    let error = (!succeeded).then(|| {
                        client
                            .tab(id)
                            .and_then(|tab| tab.notice.as_ref())
                            .map_or_else(
                                || tr!("app.error.repository_unavailable"),
                                |notice| notice.message.text(),
                            )
                    });
                    let _ = form.update(cx, |this, cx| {
                        this.modal.set_busy(false);
                        if let Some(error) = error {
                            this.error = Some(Message::detail("app.error.operation", error));
                        }
                        this.reload(window, cx);
                        cx.notify();
                        window.refresh();
                    });
                },
                window,
                cx,
            )
        });
        let failure = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(Message::detail("app.error.operation", error)),
        };
        if let Some(error) = failure {
            self.modal.set_busy(false);
            self.error = Some(error);
        }
        cx.notify();
        window.refresh();
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = PathBuf::from(self.path.read(cx).value().trim());
        let branch = self.branch.read(cx).value().trim().to_owned();
        self.run(
            RepoOperation::AddWorktree(NewWorktree {
                path,
                branch,
                create_branch: self.create_branch,
            }),
            window,
            cx,
        );
    }

    fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let modal = self.modal.clone();
        if let Err(error) = self.client.update(cx, |client, cx| {
            client.close_content_modal(&modal, window, cx);
            client.open_repo(OpenRepo::Open(path), window, cx);
        }) {
            self.error = Some(Message::detail("app.error.operation", error));
            cx.notify();
        }
    }
}

impl Render for WorktreeForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let busy = self.modal.is_busy();
        div()
            .id("worktrees-form")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(p.text_muted))
                            .child(tr!("app.worktrees.hint")),
                    )
                    .child(button(
                        "refresh-worktrees",
                        tr!("app.conflicts.reload"),
                        Tone::Outline,
                        !busy,
                        cx,
                        cx.listener(|this, _, window, cx| this.reload(window, cx)),
                    )),
            )
            .when_some(self.error.as_ref(), |body, error| {
                body.child(error_text(error, cx))
            })
            .child(match &self.entries {
                Load::Pending => div().child(tr!("app.worktrees.loading")).into_any_element(),
                Load::Failed(error) => error_text(error, cx).into_any_element(),
                Load::Ready(entries) => uniform_list(
                    "worktree-list",
                    entries.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let Load::Ready(entries) = &this.entries else {
                            return Vec::new();
                        };
                        let p = palette(cx);
                        let busy = this.modal.is_busy();
                        range
                            .map(|index| {
                                let entry = &entries[index];
                                let opened = this.opened.contains(&entry.path);
                                let removable = !busy
                                    && !opened
                                    && !entry.is_main
                                    && !entry.is_current
                                    && !entry.bare
                                    && !entry.dirty
                                    && entry.locked.is_none()
                                    && entry.prunable.is_none();
                                let mut states = Vec::new();
                                if entry.is_main {
                                    states.push(tr!("app.worktrees.main"));
                                }
                                if entry.is_current {
                                    states.push(tr!("app.worktrees.current"));
                                }
                                if opened {
                                    states.push(tr!("app.worktrees.opened"));
                                }
                                if entry.dirty {
                                    states.push(tr!("app.worktrees.modified"));
                                }
                                if entry.bare {
                                    states.push(tr!("app.worktrees.bare"));
                                }
                                if let Some(reason) = &entry.locked {
                                    states.push(tr!("app.worktrees.locked", reason = reason));
                                }
                                if let Some(reason) = &entry.prunable {
                                    states.push(tr!("app.worktrees.prunable", reason = reason));
                                }
                                let branch = entry
                                    .branch
                                    .clone()
                                    .unwrap_or_else(|| tr!("app.worktrees.detached"));
                                let head = entry
                                    .head
                                    .as_deref()
                                    .map(ritkraken_git::short_id)
                                    .unwrap_or("");
                                let path = entry.path.clone();
                                let remove = entry.clone();
                                div()
                                    .id(("worktree-entry", index))
                                    .w_full()
                                    .h(px(94.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .border_b_1()
                                    .border_color(rgb(p.border))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.))
                                            .child(
                                                div()
                                                    .truncate()
                                                    .child(path.to_string_lossy().into_owned()),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_color(rgb(p.text_muted))
                                                    .child(format!("{branch}  ·  {head}")),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_color(rgb(p.text_muted))
                                                    .child(states.join("  ·  ")),
                                            ),
                                    )
                                    .child(button(
                                        ("open-worktree", index),
                                        tr!("app.worktrees.open"),
                                        Tone::Outline,
                                        !busy && !entry.bare && entry.prunable.is_none(),
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            this.open(path.clone(), window, cx)
                                        }),
                                    ))
                                    .child(button(
                                        ("remove-worktree", index),
                                        tr!("app.worktrees.remove"),
                                        Tone::Red,
                                        removable,
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            this.run(
                                                RepoOperation::RemoveWorktree(remove.clone()),
                                                window,
                                                cx,
                                            )
                                        }),
                                    ))
                            })
                            .collect()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0()
                .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(rgb(p.border))
                    .pt(px(12.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("app.worktrees.create")),
                    )
                    .child(Input::new(&self.path).disabled(busy).w_full())
                    .child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .items_center()
                            .child(Input::new(&self.branch).disabled(busy).flex_1().min_w_0())
                            .child(
                                Checkbox::new("worktree-new-branch")
                                    .label(tr!("app.worktrees.new_branch"))
                                    .checked(self.create_branch)
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, checked, _, cx| {
                                        this.create_branch = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(button(
                        "create-worktree",
                        tr!("app.worktrees.create"),
                        Tone::Green,
                        !busy
                            && !self.path.read(cx).value().trim().is_empty()
                            && !self.branch.read(cx).value().trim().is_empty(),
                        cx,
                        cx.listener(|this, _, window, cx| this.create(window, cx)),
                    )),
            )
    }
}
