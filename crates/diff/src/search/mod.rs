mod model;
mod reveal;
mod view;

use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, KeyBinding, KeyDownEvent, SharedString,
    Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};

use crate::DiffState;
pub(crate) use model::SearchHighlights;
pub use model::{DiffSearch, DiffSearchMatch};
pub use view::DiffSearchLabels;
pub(crate) use view::bar;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        gpui_component::input::Escape,
        Some("RitKrakenDiffSearch"),
    )]);
}

#[derive(Default)]
pub(crate) struct SearchState {
    pub(crate) open: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) input: Option<Entity<InputState>>,
    pub(crate) model: Option<DiffSearch>,
    pub(crate) reveal: bool,
    pub(crate) revision: u64,
    pub(crate) placeholder: SharedString,
    subscription: Option<Subscription>,
}

impl DiffState {
    /// Open or refocus Find. Retains the last query until the view is dropped.
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.input.is_none() {
            let input = cx.new(|cx| InputState::new(window, cx));
            self.search.subscription =
                Some(
                    cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Change) {
                            this.update_search(cx);
                        }
                    }),
                );
            self.search.input = Some(input);
        }
        self.search.open = true;
        let selected = self.selected_text(cx);
        let input = self.search.input.as_ref().unwrap().clone();
        input.update(cx, |input, cx| {
            if !selected.is_empty() && selected.len() <= 4096 && !selected.contains(['\n', '\r']) {
                input.set_value(selected, window, cx);
            }
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.update_search(cx);
    }

    pub fn is_search_open(&self) -> bool {
        self.search.open
    }

    pub(crate) fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let previous = self
            .search
            .model
            .as_ref()
            .and_then(DiffSearch::current)
            .cloned();
        self.search.model = None;
        if self.search.open {
            self.match_search(cx);
            if let (Some(model), Some(previous)) = (&mut self.search.model, previous) {
                model.restore_current(&previous);
            }
            // Refreshing files restores the source viewport separately. Keep
            // that anchor instead of navigating back to the first occurrence.
            self.search.revision = self.search.revision.wrapping_add(1);
            self.search.reveal = false;
            cx.notify();
        }
    }

    fn update_search(&mut self, cx: &mut Context<Self>) {
        self.match_search(cx);
        self.reveal_search(cx);
    }

    fn match_search(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.search.input.as_ref() else {
            return;
        };
        let query = input.read(cx).value();
        if self.search.model.is_none() {
            self.search.model = Some(DiffSearch::new(self.files()));
        }
        let model = self.search.model.as_mut().unwrap();
        model.set_query(&query, self.search.case_sensitive);
    }

    pub(crate) fn reveal_search(&mut self, cx: &mut Context<Self>) {
        if !self.search.open {
            return;
        }
        self.search.revision = self.search.revision.wrapping_add(1);
        self.search.reveal = false;
        if let Some(hit) = self.search.model.as_ref().and_then(DiffSearch::current) {
            self.scroll_to_line(hit.position.clone(), cx);
            self.search.reveal = true;
        }
        cx.notify();
    }

    pub(crate) fn move_search(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if let Some(model) = &mut self.search.model {
            model.next_match(backwards);
            self.reveal_search(cx);
        }
    }

    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.open = false;
        self.search.reveal = false;
        self.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(crate) fn search_highlights(&self) -> SearchHighlights {
        self.search
            .model
            .as_ref()
            .filter(|_| self.search.open)
            .map(DiffSearch::snapshot)
            .unwrap_or_default()
    }
}

pub(crate) fn key_down(
    state: &mut DiffState,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<DiffState>,
) {
    let key = &event.keystroke;
    let modifiers = key.modifiers;
    let primary = if cx.platform_info().is_mac() {
        modifiers.platform
    } else {
        modifiers.control
    };
    let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
    let query_focused = state
        .search
        .input
        .as_ref()
        .is_some_and(|input| input.focus_handle(cx).is_focused(window));
    if primary && !modifiers.alt && !modifiers.shift && key.key == "f" {
        state.open_search(window, cx);
    } else if plain && key.key == "f3" {
        if state.search.open {
            state.move_search(modifiers.shift, cx);
        } else {
            state.open_search(window, cx);
        }
    } else if plain && key.key == "enter" && state.search.open && query_focused {
        state.move_search(modifiers.shift, cx);
    } else if plain && key.key == "escape" && state.search.open {
        state.close_search(window, cx);
    } else {
        return;
    }
    window.prevent_default();
    cx.stop_propagation();
}
