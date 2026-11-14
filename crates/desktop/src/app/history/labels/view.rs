use std::{sync::Arc, time::Duration};

use gpui_kit::{
    base::{ElementExt as _, Popup},
    component::{scroll::ScrollableElement, tooltip::Tooltip},
    prelude::*,
    *,
};

use crate::app::{
    Client,
    refs::{RefMenu, ref_icon},
    theme::{Palette, palette},
    widgets::icon,
};

use super::{
    super::graph::{BAND_HEIGHT, CONNECTOR_OPACITY},
    RefBadge,
    hover::RefHover,
};

const BADGE_GAP: Pixels = px(3.);
const POPUP_MARGIN: Pixels = px(8.);

pub(in crate::app::history) fn ref_labels(
    labels: Option<(&str, &Arc<[RefBadge]>)>,
    color: u32,
    allow_hover: bool,
    id: u64,
    owner: WeakEntity<Client>,
    p: Palette,
) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .pl(px(7.))
        .gap(BADGE_GAP)
        .overflow_hidden()
        .when_some(labels, |element, (oid, labels)| {
            let menu = RefMenu::new(id, owner);
            let first = &labels[0];
            let badges = if labels.len() > 1 && allow_hover {
                div()
                    .id(SharedString::from(oid.to_owned()))
                    .min_w_0()
                    .child(RefStack {
                        labels: labels.clone(),
                        color,
                        menu: menu.clone(),
                    })
                    .into_any_element()
            } else {
                let primary = ref_badge(first, color, true, p)
                    .id("commit-ref")
                    .when(allow_hover, |badge| with_tooltip(badge, first));
                compact_refs(
                    menu.target(primary, first.reference.as_ref(), &first.remote_refs),
                    labels.len() - 1,
                    color,
                    false,
                    p,
                )
                .into_any_element()
            };
            element.child(badges).child(connector(color).flex_1())
        })
}

#[derive(gpui_kit::IntoElement)]
struct RefStack {
    labels: Arc<[RefBadge]>,
    color: u32,
    menu: RefMenu,
}

impl RenderOnce for RefStack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = palette(cx);
        let layout = window.use_keyed_state("ref-stack-layout", cx, |_, _| RefHover::default());
        let trigger = layout.read(cx).trigger.clone();
        let width = layout.read(cx).primary_width.clone();
        let expanded = layout.read(cx).expanded;
        let menu = self.menu.on_open_change({
            let layout = layout.downgrade();
            move |open, window, cx| {
                let _ = layout.update(cx, |layout, cx| layout.menu(open, window, cx));
            }
        });
        let first = &self.labels[0];
        let count = self.labels.len() - 1;
        let viewport = window.viewport_size();
        let height = (BAND_HEIGHT * count as f32 + px(2.) * count.saturating_sub(1) as f32)
            .min(BAND_HEIGHT * 10.)
            .min((viewport.height / 2. - BAND_HEIGHT).max(BAND_HEIGHT));
        let anchor = if trigger.get().bottom() + height + POPUP_MARGIN > viewport.height {
            Anchor::BottomLeft
        } else {
            Anchor::TopLeft
        };
        let primary = ref_badge(first, self.color, true, p)
            .id("commit-ref")
            .when(expanded, |badge| {
                with_tooltip(badge, first).tooltip_show_delay(Duration::from_millis(750))
            })
            .on_prepaint({
                let width = width.clone();
                move |bounds, _, _| width.set(bounds.size.width)
            });
        let trigger_row = compact_refs(
            menu.target(primary, first.reference.as_ref(), &first.remote_refs),
            count,
            self.color,
            expanded,
            p,
        )
        .on_prepaint(move |bounds, _, _| trigger.set(bounds));
        let trigger_row =
            trigger_row.on_hover(window.listener_for(&layout, |layout, hovered, window, cx| {
                layout.hover(false, *hovered, window, cx);
            }));
        Popup::new("ref-stack-hover", trigger_row)
            .min_w_0()
            .anchor(anchor)
            .when(expanded, |popup| {
                let max_width = (window.viewport_size().width - POPUP_MARGIN * 2.).min(px(420.));
                popup.content(
                    expanded_refs(
                        &self.labels[1..],
                        self.color,
                        width.get().min(max_width),
                        max_width,
                        height,
                        &menu,
                        p,
                    )
                    .on_hover(window.listener_for(
                        &layout,
                        |layout, hovered, window, cx| {
                            layout.hover(true, *hovered, window, cx);
                        },
                    )),
                )
            })
    }
}

fn compact_refs(
    primary: impl IntoElement,
    count: usize,
    color: u32,
    expanded: bool,
    p: Palette,
) -> Stateful<Div> {
    div()
        .id("compact-refs")
        .min_w_0()
        .overflow_hidden()
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
                        badge(color, if p.is_dark { 0.28 } else { 0.20 }, false, p)
                            .px(px(6.))
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
    menu: &RefMenu,
    p: Palette,
) -> Stateful<Div> {
    div().id("expanded-refs").pt(px(2.)).child(
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
                    .gap(px(2.))
                    .flex_shrink_0()
                    .min_w(min_width)
                    .children(labels.iter().enumerate().map(|(index, label)| {
                        let badge = ref_badge(label, color, false, p)
                            .id(("expanded-ref", index))
                            .flex_shrink_0()
                            .when(!label.remote_refs.is_empty(), |badge| {
                                with_tooltip(badge, label)
                            });
                        div()
                            .rounded_full()
                            .bg(rgb(p.canvas))
                            .self_start()
                            .child(menu.target(badge, label.reference.as_ref(), &label.remote_refs))
                    })),
            )
            .overflow_scrollbar(),
    )
}

fn ref_badge(label: &RefBadge, color: u32, compact: bool, p: Palette) -> Div {
    let opacity = if label.checked_out {
        if p.is_dark { 0.42 } else { 0.34 }
    } else if p.is_dark {
        0.24
    } else {
        0.18
    };
    badge(color, opacity, label.checked_out, p)
        .when(label.checked_out, |badge| {
            badge.child(badge_icon("icons/check.svg", p))
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
        .children(label.icons().map(|kind| badge_icon(ref_icon(kind), p)))
}

fn with_tooltip(badge: Stateful<Div>, label: &RefBadge) -> Stateful<Div> {
    let tooltip = label.tooltip();
    badge.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}

fn badge_icon(path: &'static str, p: Palette) -> Svg {
    icon(path, p.text).size(px(12.))
}

fn badge(color: u32, opacity: f32, active: bool, p: Palette) -> Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .h(BAND_HEIGHT)
        .px(px(7.))
        .flex()
        .items_center()
        .gap(BADGE_GAP)
        .rounded_full()
        .when(active, |pill| {
            pill.border_1().border_color(rgb(color).opacity(0.72))
        })
        .text_size(px(11.5))
        .font_weight(if active {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .text_color(rgb(p.text))
        .bg(rgb(color).opacity(opacity))
}

fn connector(color: u32) -> Div {
    div().h(px(1.)).bg(rgb(color).opacity(CONNECTOR_OPACITY))
}
