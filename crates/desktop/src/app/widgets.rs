pub(super) mod loading;
pub(super) mod menu;
pub(super) mod modal;
pub(super) mod name_dialog;
pub(super) mod table;
pub(super) mod tooltip;

use gpui_kit::{prelude::*, *};

use super::{
    Message,
    theme::{Palette, palette},
};

#[derive(Clone, Copy)]
pub(super) enum Tone {
    Plain,
    Outline,
    Selected,
    Primary,
    Green,
    Red,
    Warning,
    Dark,
}

impl Tone {
    pub(super) fn selected_when(self, selected: bool) -> Self {
        if selected { Self::Selected } else { self }
    }

    pub(super) fn foreground(self, p: Palette) -> u32 {
        self.colors(p).1
    }

    fn colors(self, p: Palette) -> (Option<u32>, u32, Option<u32>, u32) {
        match self {
            Self::Plain => (None, p.text, None, p.hover),
            Self::Outline => (Some(p.surface), p.text, Some(p.border), p.hover),
            Self::Selected => (
                Some(p.accent_soft),
                p.accent_soft_fg,
                Some(p.accent_soft_border),
                p.accent_soft_hover,
            ),
            Self::Primary => (Some(p.accent), p.accent_fg, None, p.accent_hover),
            Self::Green => (
                Some(p.success_bg),
                p.success,
                Some(p.success_border),
                p.success_hover,
            ),
            Self::Red => (
                Some(p.danger_bg),
                p.danger,
                Some(p.danger_border),
                p.danger_hover,
            ),
            Self::Warning => (Some(p.warning_bg), p.warning, Some(p.warning), p.warning_bg),
            Self::Dark => (None, p.chrome_text, None, p.chrome_hover),
        }
    }
}

pub(super) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: Tone,
    enabled: bool,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    button_content(
        id,
        div().truncate().child(label.into()),
        tone,
        enabled,
        cx,
        on_click,
    )
}

pub(super) fn button_content(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    tone: Tone,
    enabled: bool,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let p = palette(cx);
    let (background, foreground, border, hover) = tone.colors(p);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .min_w_0()
        .h(px(26.))
        .px(px(10.))
        .rounded(px(8.))
        .text_size(px(12.))
        .text_color(rgb(foreground))
        .when_some(background, |element, color| element.bg(rgb(color)))
        .when_some(border, |element, color| {
            element.border_1().border_color(rgb(color))
        })
        .child(content)
        .when(enabled, |element| {
            element
                .cursor_pointer()
                .hover(|style| style.bg(rgb(hover)))
                .on_click(on_click)
        })
        .when(!enabled, |element| element.opacity(0.45))
}

pub(super) fn icon(path: &'static str, color: u32) -> Svg {
    svg()
        .path(path)
        .size(px(15.))
        .text_color(rgb(color))
        .flex_shrink_0()
}

pub(super) fn heading(label: impl Into<SharedString>, cx: &App) -> Div {
    let p = palette(cx);
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(p.text_muted))
        .child(label.into())
}

pub(super) fn note(message: impl Into<SharedString>, cx: &App) -> Div {
    let p = palette(cx);
    div()
        .px(px(12.))
        .py(px(10.))
        .text_size(px(12.))
        .text_color(rgb(p.text_muted))
        .child(message.into())
}

pub(super) fn error_text(message: &Message, cx: &App) -> Div {
    let p = palette(cx);
    div()
        .px(px(12.))
        .py(px(8.))
        .rounded(px(10.))
        .border_1()
        .border_color(rgb(p.danger_border))
        .bg(rgb(p.danger_bg))
        .text_color(rgb(p.danger))
        .text_size(px(12.))
        .child(message.text())
}
