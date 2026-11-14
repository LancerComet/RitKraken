use std::{ops::Range, sync::Arc};

use gpui_kit::{prelude::*, *};

use super::{History, layout::Rail};
use crate::app::{
    avatar::PersonAvatar,
    theme::{Palette, palette},
};

pub(super) const ROW_HEIGHT: Pixels = px(28.);
pub(super) const BAND_HEIGHT: Pixels = px(22.);
pub(super) const CONNECTOR_OPACITY: f32 = 0.38;
// One 22 px commit node with 4 px of space on each side.
pub(super) const MIN_GRAPH_WIDTH: Pixels = px(30.);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum RebaseNodeStyle {
    #[default]
    Normal,
    PlannedKeep,
    PlannedFold,
    PlannedDrop,
    TargetBase,
}

const LANE_INSET: Pixels = px(36.);
const LANE_GAP: Pixels = px(22.);
const TRAILING_SPACE: Pixels = px(24.);
const STROKE: Pixels = px(1.65);

pub(super) fn column_width(lanes: usize) -> Pixels {
    (lane_offset(lanes.saturating_sub(1)) + TRAILING_SPACE).max(px(84.))
}

pub(super) fn graph_cell(
    history: &Arc<History>,
    index: Option<usize>,
    width: Pixels,
    show_wip: bool,
    has_labels: bool,
    node_style: RebaseNodeStyle,
    p: Palette,
) -> Div {
    let data = history.clone();
    let drawing = canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            paint_graph(
                &data, index, show_wip, has_labels, node_style, bounds, window, cx,
            );
        },
    )
    .size_full();
    div()
        .relative()
        .w(width)
        .h_full()
        .flex_shrink_0()
        .overflow_hidden()
        .child(drawing)
        .when_some(index, |element, index| {
            let node = history.graph(show_wip).rows[index].node;
            let commit = &history.snapshot.commits[index];
            let left = lane_offset(node.lane) - graph_shift(width) - BAND_HEIGHT / 2.;
            let top = (ROW_HEIGHT - BAND_HEIGHT) / 2.;
            let node_view = match node_style {
                RebaseNodeStyle::PlannedDrop => div()
                    .size(BAND_HEIGHT)
                    .rounded_full()
                    .bg(rgb(p.canvas))
                    .border_1()
                    .border_color(rgb(p.danger))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(p.danger))
                    .child("×"),
                RebaseNodeStyle::PlannedFold => div()
                    .size(BAND_HEIGHT)
                    .rounded_full()
                    .bg(rgb(p.info_bg))
                    .border_1()
                    .border_color(rgb(p.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(p.info_fg))
                    .child("↓"),
                RebaseNodeStyle::PlannedKeep => div()
                    .size(BAND_HEIGHT)
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(p.accent))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(PersonAvatar::new(
                        &commit.author,
                        &commit.author_email,
                        BAND_HEIGHT - px(2.),
                        Some(p.rail_color(node.color_index)),
                    )),
                RebaseNodeStyle::TargetBase => div()
                    .size(BAND_HEIGHT)
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(p.border_strong))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(PersonAvatar::new(
                        &commit.author,
                        &commit.author_email,
                        BAND_HEIGHT - px(2.),
                        Some(p.rail_color(node.color_index)),
                    )),
                RebaseNodeStyle::Normal => div().child(PersonAvatar::new(
                    &commit.author,
                    &commit.author_email,
                    BAND_HEIGHT,
                    Some(p.rail_color(node.color_index)),
                )),
            };
            element.child(div().absolute().left(left).top(top).child(node_view))
        })
}

fn lane_offset(lane: usize) -> Pixels {
    LANE_INSET + LANE_GAP * lane as f32
}

fn graph_shift(width: Pixels) -> Pixels {
    (LANE_INSET - width / 2.).max(px(0.))
}

fn paint_graph(
    history: &History,
    index: Option<usize>,
    show_wip: bool,
    has_labels: bool,
    node_style: RebaseNodeStyle,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &App,
) {
    let p = palette(cx);
    let shift = graph_shift(bounds.size.width);
    let bounds = Bounds::new(
        bounds.origin - point(shift, px(0.)),
        size(bounds.size.width + shift, bounds.size.height),
    );
    let visible = bounds.intersect(&window.content_mask().bounds);
    if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
        return;
    }
    let Some(index) = index else {
        paint_wip(history, bounds, window, p);
        return;
    };
    let graph = history.graph(show_wip);
    let row = &graph.rows[index];
    let top = bounds.top();
    let bottom = bounds.bottom();
    let middle = bounds.center().y;
    let node = point(bounds.left() + lane_offset(row.node.lane), middle);
    let color = rgb(p.rail_color(row.node.color_index));
    let mark_x = bounds.right() - px(2.);
    let band_opacity = if node_style == RebaseNodeStyle::PlannedDrop {
        0.05
    } else if p.is_dark {
        0.16
    } else {
        0.12
    };
    paint_band(node.x, mark_x, middle, color.opacity(band_opacity), window);
    paint_band(
        mark_x,
        bounds.right(),
        middle,
        if node_style == RebaseNodeStyle::PlannedDrop {
            color.opacity(0.35)
        } else {
            color
        },
        window,
    );
    if has_labels {
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.left(), middle - px(0.5)),
                size(node.x - bounds.left(), px(1.)),
            ),
            color.opacity(CONNECTOR_OPACITY),
        ));
    }

    let first_x = bounds.left() + LANE_INSET;
    let first_lane = ((visible.left() - first_x) / LANE_GAP).floor() as usize;
    let end_lane =
        (((visible.right() - first_x) / LANE_GAP).ceil() as usize + 1).min(graph.lane_count());
    let lanes = first_lane.min(end_lane)..end_lane;
    for rail in graph.through(index, lanes.clone()) {
        paint_vertical(
            bounds.left() + lane_offset(rail.lane),
            top,
            bottom,
            rgb(p.rail_color(rail.color_index)),
            window,
        );
    }
    let separation = if row.incoming.len() > 1 && row.outgoing.len() > 1 {
        px(2.)
    } else {
        px(0.)
    };
    paint_connections(
        &row.incoming,
        &lanes,
        bounds.left(),
        point(node.x, middle - separation),
        top,
        p,
        window,
    );
    paint_connections(
        &row.outgoing,
        &lanes,
        bounds.left(),
        point(node.x, middle + separation),
        bottom,
        p,
        window,
    );
}

fn paint_connections(
    rails: &[Rail],
    lanes: &Range<usize>,
    left: Pixels,
    node: Point<Pixels>,
    edge_y: Pixels,
    p: Palette,
    window: &mut Window,
) {
    // The nearest offscreen rail can still contribute a horizontal segment inside the clip.
    let start = rails.partition_point(|rail| rail.lane < lanes.start);
    let end = (rails.partition_point(|rail| rail.lane < lanes.end) + 1).min(rails.len());
    for rail in rails[start..end].iter().rev() {
        let x = left + lane_offset(rail.lane);
        let color = rgb(p.rail_color(rail.color_index));
        if x == node.x {
            paint_vertical(x, edge_y.min(node.y), edge_y.max(node.y), color, window);
        } else {
            let radius = px(8.).min(x - node.x).min((edge_y - node.y).abs());
            let turn_y = if edge_y < node.y {
                node.y - radius
            } else {
                node.y + radius
            };
            let mut path = PathBuilder::stroke(STROKE);
            path.move_to(point(x, edge_y));
            path.line_to(point(x, turn_y));
            path.curve_to(point(x - radius, node.y), point(x, node.y));
            path.line_to(node);
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        }
    }
}

fn paint_vertical(x: Pixels, top: Pixels, bottom: Pixels, color: Rgba, window: &mut Window) {
    window.paint_quad(fill(
        Bounds::new(point(x - STROKE / 2., top), size(STROKE, bottom - top)),
        color,
    ));
}

fn paint_band(left: Pixels, right: Pixels, middle: Pixels, color: Rgba, window: &mut Window) {
    if right <= left {
        return;
    }
    window.paint_quad(fill(
        Bounds::new(
            point(left, middle - BAND_HEIGHT / 2.),
            size(right - left, BAND_HEIGHT),
        ),
        color,
    ));
}

#[cfg(test)]
mod tests {
    use super::{BAND_HEIGHT, LANE_INSET, MIN_GRAPH_WIDTH, column_width, graph_shift};
    use gpui_kit::px;

    #[test]
    fn narrow_graph_columns_keep_one_whole_commit_node_visible() {
        for width in [MIN_GRAPH_WIDTH, px(40.), px(84.)] {
            let center = LANE_INSET - graph_shift(width);
            assert!(center - BAND_HEIGHT / 2. >= px(0.));
            assert!(center + BAND_HEIGHT / 2. <= width);
        }
        assert_eq!(graph_shift(px(84.)), px(0.));
        assert!(MIN_GRAPH_WIDTH < column_width(6));
    }
}

fn paint_wip(history: &History, bounds: Bounds<Pixels>, window: &mut Window, p: Palette) {
    let Some(row) = &history.graph(true).worktree else {
        return;
    };
    let rail = row.node;
    let color = rgb(p.rail_color(rail.color_index));
    let x = bounds.left() + lane_offset(rail.lane);
    let y = bounds.center().y;
    paint_band(x, bounds.right() - px(3.), y, color.opacity(0.42), window);
    let radius = px(10.);
    paint_dot(window, point(x, y), radius, rgb(p.canvas));
    if !row.outgoing.is_empty() {
        paint_vertical(x, y + radius, bounds.bottom(), color, window);
    }
    for index in 0..18 {
        let angle = index as f32 * std::f32::consts::TAU / 18.;
        dot(
            window,
            point(x + radius * angle.cos(), y + radius * angle.sin()),
            color,
        );
    }
}

fn dot(window: &mut Window, center: Point<Pixels>, color: Rgba) {
    paint_dot(window, center, px(0.85), color);
}

fn paint_dot(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Rgba) {
    let bounds = Bounds::new(
        center - point(radius, radius),
        size(radius * 2., radius * 2.),
    );
    window.paint_quad(fill(bounds, color).corner_radii(radius));
}
