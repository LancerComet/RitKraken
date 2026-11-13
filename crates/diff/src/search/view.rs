use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
};

use crate::DiffState;

/// Localized labels supplied by the host application.
#[derive(Clone)]
pub struct DiffSearchLabels {
    pub placeholder: SharedString,
    pub match_case: SharedString,
    pub previous: SharedString,
    pub next: SharedString,
    pub close: SharedString,
}

impl Default for DiffSearchLabels {
    fn default() -> Self {
        Self {
            placeholder: "Find in file".into(),
            match_case: "Match case".into(),
            previous: "Previous match (Shift+Enter)".into(),
            next: "Next match (Enter)".into(),
            close: "Close search (Escape)".into(),
        }
    }
}

pub(crate) fn bar(
    state: &mut DiffState,
    labels: DiffSearchLabels,
    window: &mut Window,
    cx: &mut Context<DiffState>,
) -> Option<AnyElement> {
    if !state.search.open {
        return None;
    }
    let input = state.search.input.as_ref()?.clone();
    if state.search.placeholder != labels.placeholder {
        input.update(cx, |input, cx| {
            input.set_placeholder(labels.placeholder.clone(), window, cx)
        });
        state.search.placeholder = labels.placeholder;
    }
    let has_matches = state
        .search
        .model
        .as_ref()
        .is_some_and(|model| !model.matches().is_empty());
    let count = state
        .search
        .model
        .as_ref()
        .map_or_else(|| "0/0".into(), |model| model.label());
    Some(
        h_flex()
            .id("diff-find-bar")
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .gap(px(4.))
            .px(px(8.))
            .py(px(4.))
            .border_b_1()
            .border_color(cx.theme().border)
            .text_size(px(12.))
            .child(
                gpui::div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&input).small().w_full().text_size(px(12.))),
            )
            .child(
                Button::new("diff-find-case")
                    .small()
                    .ghost()
                    .icon(IconName::CaseSensitive)
                    .selected(state.search.case_sensitive)
                    .tooltip(labels.match_case)
                    .on_click(cx.listener(|state, _, _, cx| {
                        state.search.case_sensitive = !state.search.case_sensitive;
                        state.update_search(cx);
                    })),
            )
            .child(
                gpui::div()
                    .min_w(px(42.))
                    .text_center()
                    .flex_shrink_0()
                    .text_color(if has_matches {
                        cx.theme().foreground
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(count),
            )
            .child(
                Button::new("diff-find-prev")
                    .small()
                    .ghost()
                    .icon(IconName::ArrowUp)
                    .tooltip(labels.previous)
                    .disabled(!has_matches)
                    .on_click(cx.listener(|state, _, _, cx| state.move_search(true, cx))),
            )
            .child(
                Button::new("diff-find-next")
                    .small()
                    .ghost()
                    .icon(IconName::ArrowDown)
                    .tooltip(labels.next)
                    .disabled(!has_matches)
                    .on_click(cx.listener(|state, _, _, cx| state.move_search(false, cx))),
            )
            .child(
                Button::new("diff-find-close")
                    .small()
                    .ghost()
                    .icon(IconName::Close)
                    .tooltip(labels.close)
                    .on_click(cx.listener(|state, _, window, cx| state.close_search(window, cx))),
            )
            .into_any_element(),
    )
}
