mod actions;
#[cfg(test)]
mod filter_tests;
mod model;
mod sources;
#[cfg(test)]
mod tests;
mod view;

use gpui_kit::{
    component::input::{InputEvent, InputState},
    *,
};
use ritkraken_git::CommitSearch;

use crate::{
    app::{
        Client, Message,
        modals::{ContentModal, ModalHandle},
    },
    i18n::tr,
};
use model::Candidate;
pub(in crate::app) use model::{Command, CommandContext};

struct PaletteForm {
    client: WeakEntity<Client>,
    repo_id: Option<u64>,
    modal: ModalHandle,
    input: Entity<InputState>,
    candidates: Vec<Candidate>,
    filtered: Vec<usize>,
    selected: usize,
    scroll: UniformListScrollHandle,
    pending: usize,
    error: Option<Message>,
    _input: Subscription,
}

impl Client {
    pub(in crate::app) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_is_present() || self.session.opening {
            return;
        }
        let tab = self.current();
        let repo_id = tab.map(|tab| tab.id);
        let repository = tab.map(|tab| (tab.repo.clone(), tab.history.data.snapshot.clone()));
        let candidates = sources::immediate(tab.map(CommandContext::from));
        let client = cx.weak_entity();
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let form = cx.new(|cx| {
            let input = cx
                .new(|cx| InputState::new(window, cx).placeholder(tr!("app.palette.placeholder")));
            let subscription = cx.subscribe_in(
                &input,
                window,
                |this: &mut PaletteForm, _, event: &InputEvent, window, cx| {
                    if !this.modal.is_open() {
                        return;
                    }
                    match event {
                        InputEvent::Change => {
                            this.refilter(cx);
                            this.selected = 0;
                            this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                            cx.notify();
                        }
                        InputEvent::PressEnter {
                            secondary: false,
                            shift: false,
                        } => {
                            this.execute(window, cx);
                        }
                        _ => {}
                    }
                },
            );
            let filtered = model::filter(&candidates, "");
            PaletteForm {
                client,
                repo_id,
                modal: handle,
                input,
                candidates,
                filtered,
                selected: 0,
                scroll: UniformListScrollHandle::default(),
                pending: 0,
                error: None,
                _input: subscription,
            }
        });
        let focus = form.read(cx).input.read(cx).focus_handle(cx);
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.palette.title"),
                form.clone(),
                size(px(760.), px(620.)),
                false,
            )
            .focus(focus),
            window,
            cx,
        );
        if let Some((repo, snapshot)) = repository {
            cx.defer_in(window, move |_, window, cx| {
                form.update(cx, |form, cx| {
                    if !form.modal.is_open() {
                        return;
                    }
                    form.pending = 2;
                    let commits_repo = repo.clone();
                    Client::spawn_background(
                        window,
                        cx,
                        move || commits_repo.search_commits(&CommitSearch::default()),
                        |form, result, _, cx| {
                            form.loaded(result.map(sources::commits).map_err(Message::from), cx);
                        },
                    );
                    Client::spawn_background(
                        window,
                        cx,
                        move || {
                            let files = match &snapshot.head {
                                Some(head) => repo.committed_files(head)?,
                                None => Vec::new(),
                            };
                            Ok::<_, ritkraken_git::Error>(sources::files(&snapshot, files))
                        },
                        |form, result, _, cx| form.loaded(result.map_err(Message::from), cx),
                    );
                    cx.notify();
                });
            });
        }
    }
}

impl PaletteForm {
    fn refilter(&mut self, cx: &App) {
        self.filtered = model::filter(&self.candidates, &self.input.read(cx).value());
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn loaded(&mut self, result: Result<Vec<Candidate>, Message>, cx: &mut Context<Self>) {
        if !self.modal.is_open() {
            return;
        }
        self.pending = self.pending.saturating_sub(1);
        match result {
            Ok(candidates) => self.candidates.extend(candidates),
            Err(message) => self.error = Some(message),
        }
        self.refilter(cx);
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !self.modal.is_open() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.filtered.len().saturating_sub(1));
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }
}
