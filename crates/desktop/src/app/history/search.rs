use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::*,
    *,
};
use ritkraken_git::{Commit, CommitSearch, Repo};

use crate::{
    app::{
        Client, Escape, Load, Message, Submit,
        modals::{ContentModal, ModalHandle},
        theme::palette,
        widgets::{Tone, button, error_text},
    },
    i18n::tr,
};

struct CommitSearchForm {
    repo_id: u64,
    repo: Repo,
    client: WeakEntity<Client>,
    inputs: [Entity<InputState>; 6],
    modal: ModalHandle,
    results: Option<Load<Vec<Commit>>>,
    generation: u64,
    _inputs: [Subscription; 6],
}

impl Client {
    pub(in crate::app) fn open_commit_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_is_present() {
            return;
        }
        let Some(tab) = self.current().filter(|tab| !tab.busy) else {
            return;
        };
        let id = tab.id;
        let repo = tab.repo.clone();
        let client = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let form = cx.new(|cx| {
            let inputs = std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx)));
            let subscriptions = inputs.each_ref().map(|input| {
                cx.subscribe(
                    input,
                    |this: &mut CommitSearchForm, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.generation += 1;
                            this.results = None;
                            cx.notify();
                        }
                    },
                )
            });
            CommitSearchForm {
                repo_id: id,
                repo,
                client,
                inputs,
                modal: handle,
                results: None,
                generation: 0,
                _inputs: subscriptions,
            }
        });
        let focus = form.read(cx).inputs[0].read(cx).focus_handle(cx);
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.search.title"),
                form,
                size(px(800.), px(720.)),
                false,
            )
            .focus(focus),
            window,
            cx,
        );
    }
}

impl CommitSearchForm {
    fn search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let [text, author, path, branch, since, until] = self
            .inputs
            .each_ref()
            .map(|input| input.read(cx).value().trim().to_owned());
        let query = CommitSearch {
            text,
            author,
            path,
            branch,
            since,
            until,
        };
        let repo = self.repo.clone();
        self.generation += 1;
        let generation = self.generation;
        self.results = Some(Load::Pending);
        Client::spawn_background(
            window,
            cx,
            move || repo.search_commits(&query),
            move |this, result, _, cx| {
                if this.modal.is_open() && this.generation == generation {
                    this.results = Some(result.into());
                    cx.notify();
                }
            },
        );
        cx.notify();
    }

    fn reveal(&mut self, commit: String, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.repo_id;
        let modal = self.modal.clone();
        let result = self.client.update(cx, |client, cx| {
            client.close_content_modal(&modal, window, cx);
            client.reveal_commit(id, commit, window, cx);
        });
        if let Err(error) = result {
            self.results = Some(Load::Failed(Message::detail("app.error.operation", error)));
            cx.notify();
        }
    }
}

impl Render for CommitSearchForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .id("commit-search-form")
            .key_context("CommitOperation")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .children(
                [
                    ["app.search.text", "app.search.author"],
                    ["app.search.path", "app.search.branch"],
                    ["app.search.since", "app.search.until"],
                ]
                .into_iter()
                .enumerate()
                .map(|(row, labels)| {
                    div()
                        .flex()
                        .gap(px(12.))
                        .children(labels.into_iter().enumerate().map(|(column, label)| {
                            let input = &self.inputs[row * 2 + column];
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(div().text_color(rgb(p.text_muted)).child(tr!(label)))
                                .child(Input::new(input).w_full().text_size(px(12.)))
                        }))
                }),
            )
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
                            .child(tr!("app.search.hint")),
                    )
                    .child(button(
                        "search-commits",
                        tr!("app.search.run"),
                        Tone::Primary,
                        !matches!(self.results, Some(Load::Pending)),
                        cx,
                        cx.listener(|this, _, window, cx| this.search(window, cx)),
                    )),
            )
            .child(match &self.results {
                None => div().child(tr!("app.search.ready")).into_any_element(),
                Some(Load::Pending) => div().child(tr!("app.search.loading")).into_any_element(),
                Some(Load::Failed(error)) => error_text(error, cx).into_any_element(),
                Some(Load::Ready(commits)) if commits.is_empty() => {
                    div().child(tr!("app.search.empty")).into_any_element()
                }
                Some(Load::Ready(commits)) => {
                    let count = commits.len();
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(tr!("app.search.count", count = count))
                        .child(
                            uniform_list(
                                "commit-search-results",
                                count,
                                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                    let Some(Load::Ready(commits)) = &this.results else {
                                        return Vec::new();
                                    };
                                    range
                                        .map(|index| {
                                            let commit = &commits[index];
                                            let oid = commit.id.clone();
                                            let p = palette(cx);
                                            div()
                                                .id(("search-result", index))
                                                .h(px(52.))
                                                .px(px(8.))
                                                .flex()
                                                .flex_col()
                                                .justify_center()
                                                .border_b_1()
                                                .border_color(rgb(p.border))
                                                .cursor_pointer()
                                                .hover(|style| style.bg(rgb(p.hover)))
                                                .child(
                                                    div().truncate().child(commit.subject.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_color(rgb(p.text_muted))
                                                        .child(format!(
                                                            "{}  ·  {}  ·  {}",
                                                            ritkraken_git::short_id(&oid),
                                                            commit.author,
                                                            commit.date
                                                        )),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.reveal(oid.clone(), window, cx);
                                                    },
                                                ))
                                        })
                                        .collect()
                                }),
                            )
                            .flex_1()
                            .min_h_0(),
                        )
                        .into_any_element()
                }
            })
    }
}
