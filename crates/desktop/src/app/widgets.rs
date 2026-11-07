pub(super) mod menu;
pub(super) mod table;
pub(super) mod tooltip;

use gpui_kit::{prelude::*, *};

use super::Message;

pub(super) const WHITE: u32 = 0xffffff;
pub(super) const PAPER: u32 = 0xf6f7f8;
pub(super) const HEADER: u32 = 0xf1f2f4;
pub(super) const LINE: u32 = 0xd7d9de;
pub(super) const TEXT: u32 = 0x252932;
pub(super) const MUTED: u32 = 0x646b7c;
pub(super) const SUBTLE: u32 = 0x959ba8;
pub(super) const NAV: u32 = 0x363b51;
pub(super) const TOOLBAR: u32 = 0x444b63;
pub(super) const NAV_TEXT: u32 = 0xe0e4f0;
pub(super) const SELECT: u32 = 0xdde6f8;
pub(super) const HOVER: u32 = 0xeef2f9;
pub(super) const TEAL: u32 = 0x1597b0;
pub(super) const GREEN: u32 = 0x307542;
pub(super) const GREEN_BG: u32 = 0xe5f2e5;
pub(super) const RED: u32 = 0xad3e42;

#[derive(Clone, Copy)]
pub(super) enum Tone {
    Plain,
    Outline,
    Selected,
    Green,
    Red,
    Dark,
}

impl Tone {
    pub(super) fn selected_when(self, selected: bool) -> Self {
        if selected { Self::Selected } else { self }
    }

    pub(super) fn foreground(self) -> u32 {
        self.colors().1
    }

    fn colors(self) -> (Option<u32>, u32, Option<u32>, u32) {
        match self {
            Self::Plain => (None, TEXT, None, HOVER),
            Self::Outline => (Some(WHITE), TEXT, Some(LINE), HOVER),
            Self::Selected => (Some(SELECT), 0x234773, Some(0xb3c8eb), 0xd1dff6),
            Self::Green => (Some(GREEN_BG), GREEN, Some(0xa7cba9), 0xd3ead4),
            Self::Red => (Some(0xfbe9e9), RED, Some(0xdfa0a2), 0xf4dada),
            Self::Dark => (None, NAV_TEXT, None, 0x555f7c),
        }
    }
}

pub(super) fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    button_content(
        id,
        div().truncate().child(label.into()),
        tone,
        enabled,
        on_click,
    )
}

pub(super) fn button_content(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (background, foreground, border, hover) = tone.colors();
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .min_w_0()
        .h(px(25.))
        .px(px(8.))
        .rounded(px(3.))
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
        .when(!enabled, |element| element.opacity(0.48))
}

pub(super) fn modal_backdrop() -> Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x20253655))
}

pub(super) fn icon(path: &'static str, color: u32) -> Svg {
    svg()
        .path(path)
        .size(px(15.))
        .text_color(rgb(color))
        .flex_shrink_0()
}

pub(super) fn heading(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(label.into())
}

pub(super) fn note(message: impl Into<SharedString>) -> Div {
    div()
        .px(px(12.))
        .py(px(10.))
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .child(message.into())
}

pub(super) fn error_text(message: &Message) -> Div {
    div()
        .px(px(12.))
        .py(px(8.))
        .bg(rgb(0xfcebec))
        .text_color(rgb(RED))
        .text_size(px(12.))
        .child(message.text())
}
