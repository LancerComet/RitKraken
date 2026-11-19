use std::collections::HashSet;
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
    graph::{BAND_HEIGHT, MIN_GRAPH_WIDTH, ROW_HEIGHT, RebaseNodeStyle, column_width, graph_cell},
    labels::ref_labels,
    rebase::{
        DraggedRebaseRow, PlannedRowMeta, RebaseDrop, RebaseEdit, RebaseFixup, RebaseMoveDown,
        RebaseMoveUp, RebasePick, RebaseReword, RebaseSquash, RebaseViewMode,
    },
};
use crate::app::{
    Client, NextRow, PreviousRow, RepoTab,
    changes::SummaryStyle,
    theme::{Palette, palette},
    widgets::{
        bento_card,
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
        let p = palette(cx);
        let id = tab.id;
        let history = tab.active_history().clone();
        let selected = tab.history.selected.clone();
        let show_wip = tab.show_wip();
        let columns = columns(history.graph(show_wip).lane_count());
        let widths = tab.history.columns.widths(&columns, cx);
        let count = history.graph(show_wip).rows.len() + usize::from(show_wip);
        let view = cx.weak_entity();
        let focus = tab.history.focus.clone();
        let allow_ref_hover =
            self.manager.mode.is_none() && !self.branch_editor.is_editing() && tab.rebase.is_none();
        let inline_branch = self.branch_editor.for_tab(id);
        let busy = tab.busy;
        let rebase_mode = tab.rebase.as_ref().map(|session| session.mode);
        let rebase_base = tab.rebase.as_ref().map(|session| session.plan.base.clone());
        let rebase_count = tab
            .rebase
            .as_ref()
            .map_or(0, |session| session.plan.entries.len());
        let rebase_meta = tab
            .rebase
            .as_ref()
            .map(|session| Arc::new(session.projected.row_meta.clone()));
        let rebase_edited = tab.rebase.as_ref().map(|session| {
            let initial_messages: std::collections::HashMap<&str, &str> = session
                .initial_plan
                .entries
                .iter()
                .map(|entry| (entry.commit.id.as_str(), entry.message.as_str()))
                .collect();
            Arc::new(
                session
                    .plan
                    .entries
                    .iter()
                    .filter(|entry| {
                        initial_messages
                            .get(entry.commit.id.as_str())
                            .is_some_and(|orig| *orig != entry.message)
                    })
                    .map(|entry| entry.commit.id.clone())
                    .collect::<HashSet<String>>(),
            )
        });
        let banner = if let Some(session) = &tab.rebase {
            Some(super::rebase::rebase_planning_bar(tab, session, cx))
        } else if let Some(kind) = history
            .snapshot
            .sequence
            .filter(|kind| *kind != git::SequenceKind::Other)
        {
            Some(super::rebase::sequence_control_bar(tab, kind, cx))
        } else {
            None
        };
        let lazy = self.saved.preferences.lazy_history && history.snapshot.history_limited;
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
                    let planned_meta = selection
                        .as_deref()
                        .and_then(|sel| rebase_meta.as_ref().and_then(|map| map.get(sel).cloned()));
                    let is_target_base = selection
                        .as_deref()
                        .is_some_and(|sel| rebase_base.as_deref() == Some(sel));
                    let node_style = match &planned_meta {
                        Some(meta) if meta.action == git::RebaseAction::Drop => {
                            RebaseNodeStyle::PlannedDrop
                        }
                        Some(meta)
                            if matches!(
                                meta.action,
                                git::RebaseAction::Squash | git::RebaseAction::Fixup
                            ) =>
                        {
                            RebaseNodeStyle::PlannedFold
                        }
                        Some(_) => RebaseNodeStyle::PlannedKeep,
                        None if is_target_base => RebaseNodeStyle::TargetBase,
                        None => RebaseNodeStyle::Normal,
                    };
                    let edited = planned_meta.as_ref().is_some_and(|meta| {
                        rebase_edited
                            .as_ref()
                            .is_some_and(|set| set.contains(&meta.oid))
                    });
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
                        planned_meta.as_ref(),
                        rebase_mode,
                        rebase_count,
                        edited,
                        node_style,
                        busy,
                        p,
                        cx,
                    );
                    let stash = selection
                        .as_deref()
                        .and_then(|target| history.stash(target))
                        .cloned();
                    let menu = selection.clone().map(|target| CommitMenu {
                        id,
                        target,
                        owner: view.clone(),
                    });
                    let view_for_wip = view.clone();
                    let view_for_drop = view.clone();
                    let view_for_menu = view.clone();
                    let view = view.clone();
                    let focus = focus.clone();
                    let selection_for_click = selection.clone();
                    let activate = Rc::new(move |window: &mut Window, cx: &mut App| {
                        let _ = view.update(cx, |this, cx| {
                            focus.focus(window, cx);
                            if this
                                .tab(id)
                                .is_some_and(|tab| tab.history.selected != selection_for_click)
                            {
                                this.select(id, selection_for_click.clone(), None, window, cx);
                            }
                        });
                    });
                    let right_click = activate.clone();
                    let mut element = element
                        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                            right_click(window, cx)
                        })
                        .on_click(move |_, window, cx| activate(window, cx));
                    if let (Some(meta), Some(mode)) = (planned_meta, rebase_mode) {
                        if !busy && mode == RebaseViewMode::TreeEdit {
                            let drag_row = DraggedRebaseRow {
                                entry_index: meta.entry_index,
                                oid: meta.oid.clone(),
                                action: meta.action,
                                subject: meta
                                    .message
                                    .lines()
                                    .find(|line| !line.trim().is_empty())
                                    .unwrap_or_default()
                                    .trim()
                                    .to_owned(),
                            };
                            let target_entry = meta.entry_index;
                            let accent = p.accent;
                            let accent_soft = p.accent_soft;
                            element = element
                                .on_drag(drag_row, |row, _, _, cx| {
                                    super::rebase::drag_preview(row, cx)
                                })
                                .drag_over::<DraggedRebaseRow>(move |style, dragged, _, _| {
                                    if dragged.entry_index == target_entry {
                                        style
                                    } else if dragged.entry_index < target_entry {
                                        style
                                            .border_t_2()
                                            .border_color(rgb(accent))
                                            .bg(rgb(accent_soft))
                                    } else {
                                        style
                                            .border_b_2()
                                            .border_color(rgb(accent))
                                            .bg(rgb(accent_soft))
                                    }
                                })
                                .on_drop::<DraggedRebaseRow>(move |dragged, window, cx| {
                                    let _ = view_for_drop.update(cx, |this, cx| {
                                        this.move_rebase_entry_to(
                                            id,
                                            dragged.entry_index,
                                            target_entry,
                                            window,
                                            cx,
                                        )
                                    });
                                });
                        }
                        element
                            .context_menu(move |popup, window, _| {
                                super::rebase::planned_context_menu(
                                    id,
                                    view_for_menu.clone(),
                                    meta.clone(),
                                    mode,
                                    rebase_count,
                                    busy,
                                    popup,
                                    window,
                                )
                            })
                            .into_any_element()
                    } else if rebase_mode.is_some() {
                        let copy_id = selection.map(|sel| super::rebase::real_oid(&sel).to_owned());
                        element
                            .context_menu(move |popup, window, _| {
                                let menu = crate::app::widgets::menu::menu_style(popup, window);
                                if let Some(oid) = copy_id.clone() {
                                    menu.item(
                                        crate::app::widgets::menu::PopupMenuItem::new(tr!(
                                            "app.commit_menu.copy_hash"
                                        ))
                                        .on_click(
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    oid.clone(),
                                                ));
                                            },
                                        ),
                                    )
                                } else {
                                    menu
                                }
                            })
                            .into_any_element()
                    } else if let Some(stash) = stash {
                        let owner = view_for_wip.clone();
                        element
                            .context_menu(move |popup, window, cx| {
                                super::stash_menu::stash_menu(id, &stash, &owner, popup, window, cx)
                            })
                            .into_any_element()
                    } else if let Some(menu) = menu {
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
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
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
            .horizontal_scrollbar(&tab.history.horizontal_scroll);
        bento_card(p)
            .id(("graph-view", id))
            .key_context(crate::app::shortcuts::graph_context(
                rebase_mode == Some(RebaseViewMode::TreeEdit),
            ))
            .track_focus(&tab.history.focus)
            .on_action(
                cx.listener(|this, _: &NextRow, window, cx| this.move_selection(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousRow, window, cx| {
                    this.move_selection(-1, window, cx)
                }),
            )
            .on_action(cx.listener(move |this, _: &RebasePick, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Pick, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseReword, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Reword, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseEdit, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Edit, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseSquash, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Squash, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseFixup, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Fixup, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseDrop, window, cx| {
                this.apply_rebase_shortcut(id, git::RebaseAction::Drop, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseMoveUp, window, cx| {
                this.move_selected_rebase_visual(id, -1, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &RebaseMoveDown, window, cx| {
                this.move_selected_rebase_visual(id, 1, window, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .text_color(rgb(p.text))
            .when_some(banner, |element, bar| element.child(bar))
            .child(body)
    }
}

#[allow(clippy::too_many_arguments)]
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
    planned_meta: Option<&PlannedRowMeta>,
    rebase_mode: Option<RebaseViewMode>,
    rebase_count: usize,
    edited: bool,
    node_style: RebaseNodeStyle,
    busy: bool,
    p: Palette,
    cx: &App,
) -> Stateful<Div> {
    let commit = index.map(|index| &history.snapshot.commits[index]);
    let labels = commit.and_then(|commit| {
        history
            .labels_for(&commit.id)
            .map(|labels| (commit.id.as_str(), labels))
    });
    let color = index.map_or(p.accent, |index| {
        p.rail_color(history.graph(show_wip).rows[index].node.color_index)
    });
    let refs = if let Some((_, input)) =
        inline_branch.filter(|(target, _)| commit.is_some_and(|commit| commit.id == *target))
    {
        super::branch_editor::field(input, busy)
    } else {
        ref_labels(labels, color, allow_ref_hover, id, owner.clone(), p)
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
        .bg(rgb(if selected { p.accent_soft } else { p.canvas }))
        .hover(|style| {
            style.bg(rgb(if selected {
                p.accent_soft_hover
            } else {
                p.hover
            }))
        })
        .child(widths.cell(0, refs))
        .child(widths.cell(
            1,
            graph_cell(
                history,
                index,
                widths.get(1).expect("graph column has a width"),
                show_wip,
                labels.is_some(),
                node_style,
                p,
            ),
        ));
    if let Some(commit) = commit {
        let message_cell = if let (Some(meta), Some(mode)) = (planned_meta, rebase_mode) {
            super::rebase::planned_message_column(
                id,
                owner,
                meta,
                mode,
                rebase_count,
                commit.subject.clone(),
                edited,
                busy,
                p,
                cx,
            )
        } else {
            message_column(div()).child(commit.subject.clone())
        };
        element
            .child(widths.cell(2, message_cell))
            .child(widths.cell(3, metadata(commit.author.clone(), p)))
            .child(widths.cell(4, metadata(commit.date.clone(), p)))
            .child(widths.cell(
                5,
                metadata(
                    git::short_id(super::rebase::real_oid(&commit.id)).to_owned(),
                    p,
                ),
            ))
    } else {
        element.child(widths.cell(2, wip_message(history, p)))
    }
}

fn wip_message(history: &History, p: Palette) -> Div {
    let summary = &history.worktree_summary;
    let counts = if summary.total() == 0 {
        div()
            .text_size(px(11.))
            .text_color(rgb(p.text_muted))
            .child(tr!("app.review.worktree"))
    } else {
        summary.view(SummaryStyle::Compact, p)
    };
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .h(BAND_HEIGHT)
                .flex_shrink_0()
                .flex()
                .items_center()
                .px(px(10.))
                .border_1()
                .border_color(rgb(p.border_strong))
                .rounded_full()
                .bg(rgb(p.surface_high))
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(p.text_muted))
                .child(tr!("app.history.wip")),
        )
        .child(counts)
}

fn message_column(column: Div) -> Div {
    column.pl(px(8.)).pr(px(10.)).truncate()
}

fn metadata(text: String, p: Palette) -> Div {
    div()
        .min_w_0()
        .pl(px(6.))
        .pr(px(3.))
        .truncate()
        .text_size(px(11.))
        .text_color(rgb(p.text_muted))
        .child(text)
}
