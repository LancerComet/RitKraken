use std::sync::Arc;

use gpui_kit::{component::scroll::ScrollableElement, prelude::*, *};
use ritkraken_git as git;

use super::{History, layout::Rail};
use crate::app::{
    Client, NextRow, PreviousRow, RepoTab,
    changes::distinct_file_count,
    widgets::{HEADER, HOVER, LINE, MUTED, SELECT, TEAL, WHITE, heading},
};
use crate::i18n::tr;

const ROW: f32 = 25.;
const REF_WIDTH: f32 = 132.;
const DATE_WIDTH: f32 = 117.;

impl Client {
    pub(in crate::app) fn graph_view(
        &self,
        tab: &RepoTab,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = tab.id;
        let history = tab.history.data.clone();
        let selected = tab.history.selected.clone();
        let show_wip = tab.show_wip();
        let graph_width = (history.width as f32 * 16. + 24.).clamp(64., 172.);
        let count = history.rows.len() + usize::from(show_wip);
        let view = cx.weak_entity();
        let focus = tab.history.focus.clone();
        let lazy =
            self.saved.preferences.lazy_history && history.snapshot.history_limited && !tab.busy;
        let list = uniform_list(("history", id), count, move |range, window, cx| {
            if lazy && range.start > 0 && range.end == count {
                let view = view.clone();
                window.defer(cx, move |window, cx| {
                    let _ = view.update(cx, |this, cx| this.load_older(id, window, cx));
                });
            }
            range
                .map(|row| {
                    let selection = row
                        .checked_sub(usize::from(show_wip))
                        .map(|index| history.snapshot.commits[index].id.clone());
                    let element = history_row(
                        &history,
                        selection.as_deref(),
                        row,
                        show_wip,
                        graph_width,
                        selected == selection,
                    );
                    let view = view.clone();
                    let focus = focus.clone();
                    element.on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            focus.focus(window, cx);
                            this.select(id, selection.clone(), None, window, cx);
                        });
                    })
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&tab.history.scroll)
        .size_full();
        div()
            .id(("graph-view", id))
            .key_context("CommitGraph")
            .track_focus(&tab.history.focus)
            .on_action(
                cx.listener(|this, _: &NextRow, window, cx| this.move_selection(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousRow, window, cx| {
                    this.move_selection(-1, window, cx)
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(WHITE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(22.))
                    .flex_shrink_0()
                    .bg(rgb(HEADER))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(column("app.history.ref", REF_WIDTH))
                    .child(column("app.history.graph", graph_width).flex_shrink_0())
                    .child(
                        heading(tr!("app.history.message"))
                            .min_w(px(95.))
                            .flex_1()
                            .pl(px(7.))
                            .truncate(),
                    )
                    .child(column("app.history.author", 111.))
                    .child(column("app.history.date_time", DATE_WIDTH))
                    .child(column("app.history.sha", 62.)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .vertical_scrollbar(&tab.history.scroll),
            )
    }
}

fn column(key: &'static str, width: f32) -> Div {
    heading(tr!(key))
        .w(px(width))
        .min_w_0()
        .truncate()
        .pl(px(6.))
        .border_r_1()
        .border_color(rgb(LINE))
}

fn history_row(
    history: &Arc<History>,
    oid: Option<&str>,
    row: usize,
    show_wip: bool,
    graph_width: f32,
    selected: bool,
) -> Stateful<Div> {
    let background = if selected { SELECT } else { WHITE };
    let index = row.checked_sub(usize::from(show_wip));
    let mut element = div()
        .id(("commit-row", row))
        .w_full()
        .h(px(ROW))
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(rgb(background))
        .hover(|style| style.bg(rgb(if selected { SELECT } else { HOVER })));
    let graph = history.clone();
    let drawing = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            paint_graph(&graph, index, show_wip, bounds, rgb(background), window)
        },
    )
    .w(px(graph_width))
    .h_full()
    .flex_shrink_0();
    if let (Some(oid), Some(index)) = (oid, index) {
        let commit = &history.snapshot.commits[index];
        let rail = history.rows[index].node;
        let labels: Vec<_> = history.labels_for(oid).collect();
        let first = labels
            .iter()
            .find(|reference| history.snapshot.is_current_branch(reference))
            .copied()
            .or_else(|| labels.first().copied());
        let refs = div()
            .w(px(REF_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .px(px(5.))
            .gap(px(2.))
            .when_some(first, |element, reference| {
                let current = history.snapshot.is_current_branch(reference);
                element
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .px(px(4.))
                            .rounded(px(2.))
                            .text_size(px(11.))
                            .bg(rgb(if current { 0x9edce5 } else { 0xe2eaf7 }))
                            .child(format!(
                                "{}{}",
                                if current { "✓ " } else { "" },
                                reference.name
                            )),
                    )
                    .when(labels.len() > 1, |e| {
                        e.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .text_color(rgb(MUTED))
                                .child(format!("+{}", labels.len() - 1)),
                        )
                    })
            });
        element = element
            .child(refs)
            .child(drawing)
            .child(
                div()
                    .flex_1()
                    .min_w(px(95.))
                    .truncate()
                    .pl(px(6.))
                    .border_l_1()
                    .border_color(rgb(rail.color))
                    .child(commit.subject.clone()),
            )
            .child(metadata(commit.author.clone(), 111.))
            .child(metadata(commit.date.clone(), DATE_WIDTH))
            .child(metadata(git::short_id(&commit.id).to_owned(), 62.));
    } else {
        let count = distinct_file_count(&history.snapshot.status);
        element = element
            .child(div().w(px(REF_WIDTH)).min_w_0())
            .child(drawing)
            .child(
                div().flex_1().min_w(px(95.)).truncate().pl(px(6.)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .px(px(6.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .rounded(px(3.))
                                .text_color(rgb(MUTED))
                                .child(tr!("app.history.wip")),
                        )
                        .child(div().text_color(rgb(MUTED)).text_size(px(11.)).child(
                            if count == 0 {
                                tr!("app.review.worktree")
                            } else {
                                tr!("app.history.changes", count = count)
                            },
                        )),
                ),
            )
            .child(metadata(String::new(), 111.))
            .child(metadata(String::new(), DATE_WIDTH))
            .child(metadata(String::new(), 62.));
    }
    element
}

fn metadata(text: String, width: f32) -> Div {
    div()
        .w(px(width))
        .min_w_0()
        .pl(px(6.))
        .pr(px(3.))
        .truncate()
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .child(text)
}

fn paint_graph(
    history: &History,
    index: Option<usize>,
    show_wip: bool,
    bounds: Bounds<Pixels>,
    background: Rgba,
    window: &mut Window,
) {
    let origin = bounds.origin;
    let top = origin.y;
    let bottom = top + bounds.size.height;
    let middle = top + bounds.size.height / 2.;
    let head_at_top = history
        .snapshot
        .commits
        .first()
        .is_some_and(|commit| history.snapshot.head.as_deref() == Some(commit.id.as_str()));
    let step = ((bounds.size.width - px(22.)) / history.width.max(2).saturating_sub(1) as f32)
        .min(px(16.));
    let lane_x = |lane: usize| origin.x + px(14.) + step * lane as f32;
    let paint = |start: Point<Pixels>, end: Point<Pixels>, color: u32, window: &mut Window| {
        let mut path = PathBuilder::stroke(px(1.65));
        path.move_to(start);
        if start.x == end.x {
            path.line_to(end);
        } else {
            let bend = start.y + (end.y - start.y) * 0.64;
            path.cubic_bezier_to(end, point(start.x, bend), point(end.x, bend));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(color));
        }
    };
    if let Some(row) = index.map(|index| &history.rows[index]) {
        for &Rail { lane, color } in &row.through {
            paint(
                point(lane_x(lane), top),
                point(lane_x(lane), bottom),
                color,
                window,
            );
        }
        let x = lane_x(row.node.lane);
        if row.incoming {
            paint(point(x, top), point(x, middle), row.node.color, window);
        } else if show_wip && index == Some(0) && head_at_top {
            for offset in [1., 4., 7.] {
                dot(window, point(x, top + px(offset)), px(0.85), rgb(TEAL));
            }
        }
        for parent in &row.parents {
            paint(
                point(x, middle),
                point(lane_x(parent.lane), bottom),
                parent.color,
                window,
            );
        }
        dot(window, point(x, middle), px(5.6), background);
        dot(window, point(x, middle), px(4.4), rgb(row.node.color));
    } else {
        let lane = history
            .snapshot
            .head
            .as_ref()
            .and_then(|id| history.position.get(id))
            .map_or(0, |index| history.rows[*index].node.lane);
        let x = lane_x(lane);
        if head_at_top {
            dot(window, point(x, bottom - px(3.)), px(0.85), rgb(TEAL));
        }
        for index in 0..12 {
            let angle = index as f32 * std::f32::consts::TAU / 12.;
            dot(
                window,
                point(x + px(angle.cos() * 6.), middle + px(angle.sin() * 6.)),
                px(0.85),
                rgb(TEAL),
            );
        }
    }
}

fn dot(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Rgba) {
    let bounds = Bounds::new(
        point(center.x - radius, center.y - radius),
        size(radius * 2., radius * 2.),
    );
    window.paint_quad(fill(bounds, color).corner_radii(radius));
}
