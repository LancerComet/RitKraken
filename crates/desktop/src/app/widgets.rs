pub(super) mod delete_dialog;
pub(super) mod loading;
pub(super) mod menu;
pub(super) mod modal;
pub(super) mod name_dialog;
pub(super) mod table;
pub(super) mod tooltip;

use std::rc::Rc;

use gpui_kit::{
    base::{ResizablePanelGroup, ResizeHandleRenderer, ResizeHandleState},
    prelude::*,
    *,
};

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
    let (background, foreground, border, hover) = if !enabled && matches!(tone, Tone::Primary) {
        (Some(p.surface_dim), p.text_subtle, Some(p.border), p.hover)
    } else {
        tone.colors(p)
    };
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
        .when(!enabled && !matches!(tone, Tone::Primary), |element| {
            element.opacity(0.45)
        })
}

pub(super) fn bento_card(p: Palette) -> Div {
    div()
        .rounded(px(14.))
        .border_1()
        .border_color(rgb(p.border))
        .bg(rgb(p.canvas))
        .overflow_hidden()
}

pub(super) fn bento_subcard(p: Palette) -> Div {
    div()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(p.border))
        .bg(rgb(p.surface_high))
        .overflow_hidden()
}

pub(super) fn bento_h_resizable(id: impl Into<ElementId>) -> ResizablePanelGroup {
    gpui_kit::base::h_resizable(id).with_handle_appearance(bento_resize_handle(false))
}

pub(super) fn bento_v_resizable(id: impl Into<ElementId>) -> ResizablePanelGroup {
    gpui_kit::base::v_resizable(id).with_handle_appearance(bento_resize_handle(false))
}

pub(super) fn table_h_resizable(id: impl Into<ElementId>) -> ResizablePanelGroup {
    gpui_kit::base::h_resizable(id).with_handle_appearance(bento_resize_handle(true))
}

fn bento_resize_handle(table_tick: bool) -> ResizeHandleRenderer {
    Rc::new(move |handle, _, cx| {
        let p = palette(cx);
        let axis = handle.axis();
        let state = handle.state();
        let base = div().flex_none().flex().map(|line| match axis {
            Axis::Horizontal => line.w(px(1.)).h_full().items_center(),
            _ => line.h(px(1.)).w_full().items_start().justify_center(),
        });
        let indicator = if state == ResizeHandleState::Idle {
            table_tick.then(|| {
                div()
                    .flex_none()
                    .w(px(1.))
                    .h(px(12.))
                    .rounded_full()
                    .bg(rgb(p.border))
                    .opacity(0.7)
            })
        } else {
            let length = if table_tick {
                px(16.)
            } else if state == ResizeHandleState::Dragging {
                px(44.)
            } else {
                px(28.)
            };
            let opacity = if state == ResizeHandleState::Hovered {
                0.55
            } else {
                0.85
            };
            Some(
                div()
                    .flex_none()
                    .rounded_full()
                    .bg(rgb(p.accent))
                    .opacity(opacity)
                    .map(|pill| match axis {
                        Axis::Horizontal => pill.w(px(3.)).h(length).ml(px(-1.)),
                        _ => pill.h(px(3.)).w(length).mt(px(-1.)),
                    }),
            )
        };
        Some(base.children(indicator).into_any_element())
    })
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
