use std::sync::Arc;

use gpui_kit::{
    base::InteractiveElementExt as _, component::scroll::ScrollableElement, prelude::*, *,
};
use ritkraken_git as git;

use super::{
    History,
    graph::{BAND_HEIGHT, ROW_HEIGHT, column_width, graph_cell},
    labels::ref_labels,
};
use crate::app::{
    Client, NextRow, PreviousRow, RepoTab,
    changes::SummaryStyle,
    widgets::{HEADER, HOVER, LINE, MUTED, SELECT, TEAL, WHITE, heading},
};
use crate::i18n::tr;

const HEADER_HEIGHT: Pixels = px(22.);
const REF_WIDTH: Pixels = px(156.);
const MIN_MESSAGE_WIDTH: Pixels = px(160.);
const AUTHOR_WIDTH: Pixels = px(111.);
const DATE_WIDTH: Pixels = px(117.);
const SHA_WIDTH: Pixels = px(62.);

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
        let graph_width = column_width(history.graph.lane_count());
        let count = history.graph.rows.len() + usize::from(show_wip);
        let view = cx.weak_entity();
        let focus = tab.history.focus.clone();
        let lazy =
            self.saved.preferences.lazy_history && history.snapshot.history_limited && !tab.busy;
        let mut list = uniform_list(("history", id), count, move |range, window, cx| {
            if lazy && range.start > 0 && range.end == count {
                let view = view.clone();
                window.defer(cx, move |window, cx| {
                    let _ = view.update(cx, |this, cx| this.load_older(id, window, cx));
                });
            }
            range
                .map(|row| {
                    let index = row.checked_sub(usize::from(show_wip));
                    let selection = index.map(|index| history.snapshot.commits[index].id.clone());
                    let element = history_row(
                        &history,
                        index,
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
        list.style().restrict_scroll_to_axis = Some(true);
        let header = div()
            .flex()
            .items_center()
            .h(HEADER_HEIGHT)
            .flex_shrink_0()
            .bg(rgb(HEADER))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(column("app.history.ref", REF_WIDTH).flex_shrink_0())
            .child(column("app.history.graph", graph_width).flex_shrink_0())
            .child(message_column(heading(tr!("app.history.message"))))
            .child(column("app.history.author", AUTHOR_WIDTH))
            .child(column("app.history.date_time", DATE_WIDTH))
            .child(column("app.history.sha", SHA_WIDTH));
        let table = div()
            .size_full()
            .min_w(
                REF_WIDTH + graph_width + MIN_MESSAGE_WIDTH + AUTHOR_WIDTH + DATE_WIDTH + SHA_WIDTH,
            )
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(list));
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
            .relative()
            .bg(rgb(WHITE))
            .child(
                div()
                    .id("history-columns")
                    .size_full()
                    .overflow_x_scroll()
                    .lock_scroll_axis()
                    .track_scroll(&tab.history.horizontal_scroll)
                    .child(table),
            )
            .child(
                div()
                    .absolute()
                    .top(HEADER_HEIGHT)
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .vertical_scrollbar(&tab.history.scroll),
            )
            .horizontal_scrollbar(&tab.history.horizontal_scroll)
    }
}

fn column(key: &'static str, width: Pixels) -> Div {
    heading(tr!(key))
        .w(width)
        .min_w_0()
        .truncate()
        .pl(px(6.))
        .border_r_1()
        .border_color(rgb(LINE))
}

fn history_row(
    history: &Arc<History>,
    index: Option<usize>,
    row: usize,
    show_wip: bool,
    graph_width: Pixels,
    selected: bool,
) -> Stateful<Div> {
    let commit = index.map(|index| &history.snapshot.commits[index]);
    let labels = commit.map_or(&[][..], |commit| history.labels_for(&commit.id));
    let color = index.map_or(TEAL, |index| history.graph.rows[index].node.color);
    let element = div()
        .id(("commit-row", row))
        .w_full()
        .h(ROW_HEIGHT)
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(rgb(if selected { SELECT } else { WHITE }))
        .hover(|style| style.bg(rgb(if selected { SELECT } else { HOVER })))
        .child(ref_labels(labels, color).w(REF_WIDTH).flex_shrink_0())
        .child(graph_cell(
            history,
            index,
            graph_width,
            show_wip,
            !labels.is_empty(),
        ));
    if let Some(commit) = commit {
        element
            .child(message_column(div()).child(commit.subject.clone()))
            .child(metadata(commit.author.clone(), AUTHOR_WIDTH))
            .child(metadata(commit.date.clone(), DATE_WIDTH))
            .child(metadata(git::short_id(&commit.id).to_owned(), SHA_WIDTH))
    } else {
        element.child(wip_message(history))
    }
}

fn wip_message(history: &History) -> Div {
    let summary = &history.worktree_summary;
    let counts = if summary.total() == 0 {
        div()
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .child(tr!("app.review.worktree"))
    } else {
        summary.view(SummaryStyle::Compact)
    };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(14.))
        .child(
            div()
                .w(px(96.))
                .h(BAND_HEIGHT)
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(11.))
                .border_1()
                .border_color(rgb(LINE))
                .rounded(px(3.))
                .bg(rgb(WHITE).opacity(0.3))
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(tr!("app.history.wip")),
        )
        .child(counts)
}

fn message_column(column: Div) -> Div {
    column
        .flex_1()
        .min_w(MIN_MESSAGE_WIDTH)
        .pl(px(7.))
        .truncate()
}

fn metadata(text: String, width: Pixels) -> Div {
    div()
        .w(width)
        .min_w_0()
        .pl(px(6.))
        .pr(px(3.))
        .truncate()
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .child(text)
}
