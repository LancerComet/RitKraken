use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

use gpui_kit::{
    base::{ElementExt as _, HoverCard},
    component::{scroll::ScrollableElement, tooltip::Tooltip},
    prelude::*,
    *,
};

use crate::app::{
    refs::ref_icon,
    widgets::{TEXT, WHITE, icon},
};

use super::{
    super::graph::{BAND_HEIGHT, CONNECTOR_OPACITY},
    RefBadge,
};

const BADGE_GAP: Pixels = px(3.);
const POPUP_MARGIN: Pixels = px(8.);

pub(in crate::app::history) fn ref_labels(
    labels: Option<(&str, &Arc<[RefBadge]>)>,
    color: u32,
    allow_hover: bool,
) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .pl(px(7.))
        .gap(BADGE_GAP)
        .overflow_hidden()
        .when_some(labels, |element, (oid, labels)| {
            let first = &labels[0];
            let badges = if labels.len() > 1 && allow_hover {
                div()
                    .id(SharedString::from(oid.to_owned()))
                    .min_w_0()
                    .child(RefStack {
                        labels: labels.clone(),
                        color,
                    })
                    .into_any_element()
            } else {
                let primary = ref_badge(first, color, true)
                    .id("commit-ref")
                    .when(allow_hover, |badge| with_tooltip(badge, first));
                compact_refs(primary, labels.len() - 1, color, false).into_any_element()
            };
            element.child(badges).child(connector(color).flex_1())
        })
}

#[derive(IntoElement)]
struct RefStack {
    labels: Arc<[RefBadge]>,
    color: u32,
}

#[derive(Default)]
struct StackLayout {
    expanded: bool,
    trigger: Rc<Cell<Bounds<Pixels>>>,
    primary_width: Rc<Cell<Pixels>>,
}

impl RenderOnce for RefStack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layout = window.use_keyed_state("ref-stack-layout", cx, |_, _| StackLayout::default());
        let trigger = layout.read(cx).trigger.clone();
        let width = layout.read(cx).primary_width.clone();
        let expanded = layout.read(cx).expanded;
        let first = &self.labels[0];
        let count = self.labels.len() - 1;
        let viewport = window.viewport_size();
        let height = (BAND_HEIGHT * count as f32)
            .min(BAND_HEIGHT * 10.)
            .min((viewport.height / 2. - BAND_HEIGHT).max(BAND_HEIGHT));
        let anchor = if trigger.get().bottom() + height + POPUP_MARGIN > viewport.height {
            Anchor::BottomLeft
        } else {
            Anchor::TopLeft
        };
        let primary = ref_badge(first, self.color, true)
            .id("commit-ref")
            .when(expanded, |badge| {
                with_tooltip(badge, first).tooltip_show_delay(Duration::from_millis(750))
            })
            .on_prepaint({
                let width = width.clone();
                move |bounds, _, _| width.set(bounds.size.width)
            });
        let trigger_row = compact_refs(primary, count, self.color, expanded)
            .on_prepaint(move |bounds, _, _| trigger.set(bounds));
        let layout = layout.downgrade();
        HoverCard::new("ref-stack-hover")
            .min_w_0()
            .anchor(anchor)
            .open_delay(Duration::from_millis(160))
            .close_delay(Duration::from_millis(110))
            .trigger(trigger_row)
            .on_open_change(move |open, _, cx| {
                let _ = layout.update(cx, |layout, cx| {
                    layout.expanded = *open;
                    cx.notify();
                });
            })
            .content(move |_, window, _| {
                let max_width = (window.viewport_size().width - POPUP_MARGIN * 2.).min(px(420.));
                expanded_refs(
                    &self.labels[1..],
                    self.color,
                    width.get().min(max_width),
                    max_width,
                    height,
                )
            })
    }
}

fn compact_refs(primary: Stateful<Div>, count: usize, color: u32, expanded: bool) -> Stateful<Div> {
    div()
        .id("compact-refs")
        .min_w_0()
        .flex()
        .items_center()
        .gap(BADGE_GAP)
        .child(primary)
        .when(count > 0, |element| {
            element.child(
                div()
                    .id("additional-refs")
                    .relative()
                    .flex_shrink_0()
                    .child(
                        badge(color, 0.25)
                            .child(format!("+{count}"))
                            .when(expanded, |badge| badge.opacity(0.)),
                    )
                    .when(expanded, |element| {
                        // Keep the counter's hover area while continuing the connector through it.
                        element.child(
                            connector(color)
                                .absolute()
                                .left(-BADGE_GAP)
                                .right(-BADGE_GAP)
                                .top((BAND_HEIGHT - px(1.)) / 2.),
                        )
                    }),
            )
        })
}

fn expanded_refs(
    labels: &[RefBadge],
    color: u32,
    min_width: Pixels,
    max_width: Pixels,
    height: Pixels,
) -> Stateful<Div> {
    div()
        .id("expanded-refs")
        .rounded(px(2.))
        .shadow_sm()
        .bg(rgb(WHITE))
        .child(
            div()
                .flex()
                // Let the scroll content retain its full height.
                .self_start()
                .h(height)
                .min_w(min_width)
                .max_w(max_width)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .min_w(min_width)
                        .children(labels.iter().enumerate().map(|(index, label)| {
                            ref_badge(label, color, false)
                                .id(("expanded-ref", index))
                                .flex_shrink_0()
                                .when(!label.remote_refs.is_empty(), |badge| {
                                    with_tooltip(badge, label)
                                })
                        })),
                )
                .overflow_scrollbar(),
        )
}

fn ref_badge(label: &RefBadge, color: u32, compact: bool) -> Div {
    badge(color, if label.checked_out { 0.48 } else { 0.25 })
        .when(label.checked_out, |badge| {
            badge.child(badge_icon("icons/check.svg"))
        })
        .child(
            div()
                .when(compact, |name| name.min_w_0().truncate())
                .when(!compact, |name| name.flex_shrink_0().whitespace_nowrap())
                .child(if compact {
                    label.display_name().to_owned()
                } else {
                    label.name.clone()
                }),
        )
        .children(label.icons().map(|kind| badge_icon(ref_icon(kind))))
}

fn with_tooltip(badge: Stateful<Div>, label: &RefBadge) -> Stateful<Div> {
    let tooltip = label.tooltip();
    badge.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}

fn badge_icon(path: &'static str) -> Svg {
    icon(path, TEXT).size(px(16.))
}

fn badge(color: u32, opacity: f32) -> Div {
    div()
        .min_w_0()
        .h(BAND_HEIGHT)
        .px(px(4.))
        .flex()
        .items_center()
        .gap(BADGE_GAP)
        .rounded(px(2.))
        .text_size(px(12.))
        .text_color(rgb(TEXT))
        .bg(rgb(color).opacity(opacity))
}

fn connector(color: u32) -> Div {
    div().h(px(1.)).bg(rgb(color).opacity(CONNECTOR_OPACITY))
}
