use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{input::InputState, scroll::ScrollableElement},
    prelude::*,
    *,
};
use ritkraken_git as git;

use super::{
    History,
    commit_menu::CommitMenu,
    graph::{BAND_HEIGHT, MIN_GRAPH_WIDTH, ROW_HEIGHT, column_width, graph_cell},
    labels::ref_labels,
};
use crate::app::{
    Client, NextRow, PreviousRow, RepoTab,
    changes::SummaryStyle,
    widgets::{
        HOVER, LINE, MUTED, SELECT, TEAL, WHITE,
        menu::ContextMenuExt as _,
        table::{Column, ColumnWidths, HEADER_HEIGHT},
    },
};
use crate::i18n::tr;

fn columns(lanes: usize) -> [Column; 6] {
    let graph_width = column_width(lanes);
    [
        Column {
            label: "app.history.ref",
            width: Some(px(156.)),
            minimum: px(70.),
        },
        Column {
            label: "app.history.graph",
            width: Some(graph_width),
            minimum: MIN_GRAPH_WIDTH,
        },
        Column {
            label: "app.history.message",
            width: None,
            minimum: px(160.),
        },
        Column {
            label: "app.history.author",
            width: Some(px(111.)),
            minimum: px(64.),
        },
        Column {
            label: "app.history.date_time",
            width: Some(px(117.)),
            minimum: px(80.),
        },
        Column {
            label: "app.history.sha",
            width: Some(px(62.)),
            minimum: px(56.),
        },
    ]
}

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
        let columns = columns(history.graph.lane_count());
        let widths = tab.history.columns.widths(&columns, cx);
        let count = history.graph.rows.len() + usize::from(show_wip);
        let view = cx.weak_entity();
        let focus = tab.history.focus.clone();
        let allow_ref_hover = self.manager.mode.is_none() && !self.branch_editor.is_editing();
        let inline_branch = self.branch_editor.for_tab(id);
        let busy = tab.busy;
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
                        show_wip,
                        widths,
                        selected == selection,
                        allow_ref_hover,
                        id,
                        view.clone(),
                        inline_branch.as_ref(),
                        busy,
                    );
                    let menu = selection.clone().map(|target| CommitMenu {
                        id,
                        target,
                        owner: view.clone(),
                    });
                    let view_for_wip = view.clone();
                    let view = view.clone();
                    let focus = focus.clone();
                    let activate = Rc::new(move |window: &mut Window, cx: &mut App| {
                        let _ = view.update(cx, |this, cx| {
                            focus.focus(window, cx);
                            if this
                                .tab(id)
                                .is_some_and(|tab| tab.history.selected != selection)
                            {
                                this.select(id, selection.clone(), None, window, cx);
                            }
                        });
                    });
                    let right_click = activate.clone();
                    let element = element
                        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                            right_click(window, cx)
                        })
                        .on_click(move |_, window, cx| activate(window, cx));
                    if let Some(menu) = menu {
                        element
                            .context_menu(move |popup, window, cx| menu.build(popup, window, cx))
                            .into_any_element()
                    } else {
                        let owner = view_for_wip.clone();
                        element
                            .context_menu(move |popup, window, cx| {
                                super::commit_menu::worktree_menu(id, &owner, popup, window, cx)
                            })
                            .into_any_element()
                    }
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&tab.history.scroll)
        .size_full();
        list.style().restrict_scroll_to_axis = Some(true);
        let header = tab
            .history
            .columns
            .header(("graph-columns", id), &columns, px(6.));
        let table = div()
            .size_full()
            .min_w(tab.history.columns.minimum_width(&columns, cx))
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

fn history_row(
    history: &Arc<History>,
    index: Option<usize>,
    show_wip: bool,
    widths: ColumnWidths<6>,
    selected: bool,
    allow_ref_hover: bool,
    id: u64,
    owner: WeakEntity<Client>,
    inline_branch: Option<&(String, Entity<InputState>)>,
    busy: bool,
) -> Stateful<Div> {
    let commit = index.map(|index| &history.snapshot.commits[index]);
    let labels = commit.and_then(|commit| {
        history
            .labels_for(&commit.id)
            .map(|labels| (commit.id.as_str(), labels))
    });
    let color = index.map_or(TEAL, |index| history.graph.rows[index].node.color);
    let refs = if let Some((_, input)) =
        inline_branch.filter(|(target, _)| commit.is_some_and(|commit| commit.id == *target))
    {
        super::branch_editor::field(input, busy)
    } else {
        ref_labels(labels, color, allow_ref_hover, id, owner)
    };
    let element = div()
        .id(SharedString::from(format!(
            "commit-row-{}",
            commit.map_or("wip", |commit| commit.id.as_str())
        )))
        .w_full()
        .h(ROW_HEIGHT)
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(rgb(if selected { SELECT } else { WHITE }))
        .hover(|style| style.bg(rgb(if selected { SELECT } else { HOVER })))
        .child(widths.cell(0, refs))
        .child(widths.cell(
            1,
            graph_cell(
                history,
                index,
                widths.get(1).expect("graph column has a width"),
                show_wip,
                labels.is_some(),
            ),
        ));
    if let Some(commit) = commit {
        element
            .child(widths.cell(2, message_column(div()).child(commit.subject.clone())))
            .child(widths.cell(3, metadata(commit.author.clone())))
            .child(widths.cell(4, metadata(commit.date.clone())))
            .child(widths.cell(5, metadata(git::short_id(&commit.id).to_owned())))
    } else {
        element.child(widths.cell(2, wip_message(history)))
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
    column.pl(px(7.)).truncate()
}

fn metadata(text: String) -> Div {
    div()
        .min_w_0()
        .pl(px(6.))
        .pr(px(3.))
        .truncate()
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .child(text)
}
