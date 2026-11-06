use std::{ops::Range, sync::Arc};

use gpui_kit::{prelude::*, *};

use super::{History, layout::Rail};
use crate::app::{
    avatar::PersonAvatar,
    widgets::{TEAL, WHITE},
};

pub(super) const ROW_HEIGHT: Pixels = px(28.);
pub(super) const BAND_HEIGHT: Pixels = px(22.);
pub(super) const CONNECTOR_OPACITY: f32 = 0.38;

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
) -> Div {
    let data = history.clone();
    let drawing = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            paint_graph(&data, index, show_wip, has_labels, bounds, window);
        },
    )
    .size_full();
    div()
        .relative()
        .w(width)
        .h_full()
        .flex_shrink_0()
        .child(drawing)
        .when_some(index, |element, index| {
            let node = history.graph.rows[index].node;
            let commit = &history.snapshot.commits[index];
            element.child(
                div()
                    .absolute()
                    .left(lane_offset(node.lane) - BAND_HEIGHT / 2.)
                    .top((ROW_HEIGHT - BAND_HEIGHT) / 2.)
                    .child(PersonAvatar::new(
                        &commit.author,
                        &commit.author_email,
                        BAND_HEIGHT,
                        Some(node.color),
                    )),
            )
        })
}

fn lane_offset(lane: usize) -> Pixels {
    LANE_INSET + LANE_GAP * lane as f32
}

fn paint_graph(
    history: &History,
    index: Option<usize>,
    show_wip: bool,
    has_labels: bool,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let visible = bounds.intersect(&window.content_mask().bounds);
    if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
        return;
    }
    let Some(index) = index else {
        paint_wip(history, bounds, window);
        return;
    };
    let graph = &history.graph;
    let row = &graph.rows[index];
    let top = bounds.top();
    let bottom = bounds.bottom();
    let middle = bounds.center().y;
    let node = point(bounds.left() + lane_offset(row.node.lane), middle);
    let color = rgb(row.node.color);
    let mark_x = bounds.right() - px(2.);
    paint_band(node.x, mark_x, middle, color.opacity(0.12), window);
    paint_band(mark_x, bounds.right(), middle, color, window);
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
            rgb(rail.color),
            window,
        );
    }
    if index == 0 && show_wip && history.head_index() == Some(0) {
        for offset in [1., 4., 7.] {
            dot(window, point(node.x, top + px(offset)), color);
        }
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
        window,
    );
    paint_connections(
        &row.outgoing,
        &lanes,
        bounds.left(),
        point(node.x, middle + separation),
        bottom,
        window,
    );
}

fn paint_connections(
    rails: &[Rail],
    lanes: &Range<usize>,
    left: Pixels,
    node: Point<Pixels>,
    edge_y: Pixels,
    window: &mut Window,
) {
    // The nearest offscreen rail can still contribute a horizontal segment inside the clip.
    let start = rails.partition_point(|rail| rail.lane < lanes.start);
    let end = (rails.partition_point(|rail| rail.lane < lanes.end) + 1).min(rails.len());
    for rail in rails[start..end].iter().rev() {
        let x = left + lane_offset(rail.lane);
        let color = rgb(rail.color);
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
    window.paint_quad(fill(
        Bounds::new(
            point(left, middle - BAND_HEIGHT / 2.),
            size(right - left, BAND_HEIGHT),
        ),
        color,
    ));
}

fn paint_wip(history: &History, bounds: Bounds<Pixels>, window: &mut Window) {
    let head = history.head_index();
    let rail = head.map_or(
        Rail {
            lane: 0,
            color: TEAL,
        },
        |index| history.graph.rows[index].node,
    );
    let color = rgb(rail.color);
    let x = bounds.left() + lane_offset(rail.lane);
    let y = bounds.center().y;
    paint_band(x, bounds.right() - px(3.), y, color.opacity(0.48), window);
    let radius = px(10.);
    paint_dot(window, point(x, y), radius, rgb(WHITE));
    if head == Some(0) {
        dot(window, point(x, bounds.bottom() - px(2.)), color);
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
