use gpui_kit::{
    component::{button::Button, input::Textarea, scroll::ScrollableElement},
    prelude::*,
    *,
};
use ritkraken_git::{self as git, RebaseAction};

use super::{PlannedRowMeta, RebaseSession, RebaseViewMode, action_label};
use crate::{
    app::{
        Client, RepoTab,
        theme::{Palette, palette},
        widgets::{
            Tone, button, icon,
            menu::{DropdownMenu as _, PopupMenu, PopupMenuItem, menu_style},
        },
    },
    i18n::tr,
};

#[derive(Clone, Debug)]
pub(in crate::app) struct DraggedRebaseRow {
    pub(in crate::app) entry_index: usize,
    pub(in crate::app) oid: String,
    pub(in crate::app) action: RebaseAction,
    pub(in crate::app) subject: String,
}

pub(in crate::app) struct DragPreview {
    row: DraggedRebaseRow,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (fg, bg) = action_colors(self.row.action, p);
        div()
            .max_w(px(360.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(p.accent))
            .bg(rgb(p.surface_high))
            .shadow_md()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(rgb(p.text))
            .child(
                div()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded_full()
                    .bg(rgb(bg))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(fg))
                    .child(action_label(self.row.action)),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(p.text_muted))
                    .child(git::short_id(&self.row.oid).to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.row.subject.clone()),
            )
    }
}

pub(in crate::app) fn drag_preview(row: &DraggedRebaseRow, cx: &mut App) -> Entity<DragPreview> {
    let row = row.clone();
    cx.new(|_| DragPreview { row })
}

pub(in crate::app) fn action_colors(action: RebaseAction, p: Palette) -> (u32, u32) {
    match action {
        RebaseAction::Pick => (p.text_muted, p.surface_high),
        RebaseAction::Reword => (p.accent_soft_fg, p.accent_soft),
        RebaseAction::Edit => (p.warning, p.warning_bg),
        RebaseAction::Squash => (p.info_fg, p.info_bg),
        RebaseAction::Fixup => (p.renamed, p.surface_high),
        RebaseAction::Drop => (p.danger, p.danger_bg),
    }
}

fn short_action_label(action: RebaseAction) -> String {
    match action {
        RebaseAction::Squash => tr!("app.rebase.squash_down"),
        RebaseAction::Fixup => tr!("app.rebase.fixup_down"),
        _ => action_label(action),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::app) fn planned_message_column(
    id: u64,
    owner: WeakEntity<Client>,
    meta: &PlannedRowMeta,
    mode: RebaseViewMode,
    entry_count: usize,
    subject: String,
    edited: bool,
    busy: bool,
    p: Palette,
    cx: &App,
) -> Div {
    if mode == RebaseViewMode::ResultPreview {
        let (fg, bg) = action_colors(meta.action, p);
        return div()
            .w_full()
            .min_w_0()
            .pl(px(7.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .when(meta.folded_count > 0, |element| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(rgb(p.info_bg))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(p.info_fg))
                        .child(tr!("app.rebase.folded_badge", count = meta.folded_count)),
                )
            })
            .when(meta.action != RebaseAction::Pick, |element| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(rgb(bg))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(fg))
                        .child(action_label(meta.action)),
                )
            })
            .child(div().flex_1().min_w_0().truncate().child(subject));
    }

    let entry_index = meta.entry_index;
    let action = meta.action;
    let (fg, bg) = action_colors(action, p);
    let view = owner.clone();
    let dropdown = Button::new(("rebase-row-action", entry_index))
        .label(short_action_label(action))
        .text_size(px(11.))
        .h(px(21.))
        .w(px(94.))
        .dropdown_menu(move |menu, window, _| {
            RebaseAction::ALL
                .into_iter()
                .fold(menu_style(menu, window), |menu, candidate| {
                    let view = view.clone();
                    menu.item(
                        PopupMenuItem::new(short_action_label(candidate))
                            .checked(candidate == action)
                            .on_click(move |_, window, cx| {
                                let _ = view.update(cx, |client, cx| {
                                    client.set_rebase_action(id, entry_index, candidate, window, cx)
                                });
                            }),
                    )
                })
        });

    let up_enabled = !busy && entry_index + 1 < entry_count;
    let down_enabled = !busy && entry_index > 0;
    let view_up = owner.clone();
    let view_down = owner;

    let mut subject_div = div().flex_1().min_w_0().truncate().child(subject);
    if action == RebaseAction::Drop {
        subject_div = subject_div.line_through().text_color(rgb(p.text_subtle));
    }

    div()
        .w_full()
        .min_w_0()
        .pl(px(4.))
        .pr(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.5))
                .text_color(rgb(p.text_subtle))
                .child("⋮⋮"),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(rgb(bg))
                .border_1()
                .border_color(rgb(p.border))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .w(px(6.))
                        .h(px(6.))
                        .ml(px(5.))
                        .rounded_full()
                        .bg(rgb(fg)),
                )
                .child(dropdown),
        )
        .when(edited, |element| {
            element.child(icon("icons/pencil.svg", p.accent).size(px(11.)))
        })
        .child(subject_div)
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(2.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    button(
                        ("rebase-row-up", entry_index),
                        "↑",
                        Tone::Plain,
                        up_enabled,
                        cx,
                        move |_, window, cx| {
                            cx.stop_propagation();
                            let _ = view_up.update(cx, |client, cx| {
                                client.move_rebase_entry_visual(id, entry_index, -1, window, cx)
                            });
                        },
                    )
                    .rounded_full()
                    .w(px(22.))
                    .h(px(22.))
                    .px(px(0.))
                    .text_size(px(11.)),
                )
                .child(
                    button(
                        ("rebase-row-down", entry_index),
                        "↓",
                        Tone::Plain,
                        down_enabled,
                        cx,
                        move |_, window, cx| {
                            cx.stop_propagation();
                            let _ = view_down.update(cx, |client, cx| {
                                client.move_rebase_entry_visual(id, entry_index, 1, window, cx)
                            });
                        },
                    )
                    .rounded_full()
                    .w(px(22.))
                    .h(px(22.))
                    .px(px(0.))
                    .text_size(px(11.)),
                ),
        )
}

pub(in crate::app) fn planned_context_menu(
    id: u64,
    owner: WeakEntity<Client>,
    meta: PlannedRowMeta,
    mode: RebaseViewMode,
    entry_count: usize,
    busy: bool,
    popup: PopupMenu,
    window: &mut Window,
) -> PopupMenu {
    let mut menu = menu_style(popup, window);
    if mode == RebaseViewMode::TreeEdit {
        for action in RebaseAction::ALL {
            let view = owner.clone();
            let entry_index = meta.entry_index;
            menu = menu.item(
                PopupMenuItem::new(short_action_label(action))
                    .checked(meta.action == action)
                    .disabled(busy)
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |client, cx| {
                            client.set_rebase_action(id, entry_index, action, window, cx)
                        });
                    }),
            );
        }
        let view_up = owner.clone();
        let view_down = owner.clone();
        let entry_index = meta.entry_index;
        menu = menu
            .separator()
            .item(
                PopupMenuItem::new(tr!("app.rebase.move_up"))
                    .disabled(busy || entry_index + 1 >= entry_count)
                    .on_click(move |_, window, cx| {
                        let _ = view_up.update(cx, |client, cx| {
                            client.move_rebase_entry_visual(id, entry_index, -1, window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(tr!("app.rebase.move_down"))
                    .disabled(busy || entry_index == 0)
                    .on_click(move |_, window, cx| {
                        let _ = view_down.update(cx, |client, cx| {
                            client.move_rebase_entry_visual(id, entry_index, 1, window, cx)
                        });
                    }),
            );
    }
    let oid = meta.oid;
    menu.separator().item(
        PopupMenuItem::new(tr!("app.commit_menu.copy_hash")).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(oid.clone()));
        }),
    )
}

pub(in crate::app) fn planned_message_pane(
    tab: &RepoTab,
    session: &RebaseSession,
    meta: &PlannedRowMeta,
    cx: &mut Context<Client>,
) -> Div {
    let p = palette(cx);
    let id = tab.id;
    let entry_index = meta.entry_index;
    let (fg, bg) = action_colors(meta.action, p);
    let editable = session.editing_entry == Some(entry_index)
        && ((meta.action == RebaseAction::Reword && meta.folded_count == 0)
            || (session.mode == RebaseViewMode::TreeEdit && meta.action == RebaseAction::Squash));

    let quick_button = if session.mode == RebaseViewMode::ResultPreview && meta.folded_count > 0 {
        Some(
            button(
                ("edit-in-tree", id),
                tr!("app.rebase.edit_in_tree"),
                Tone::Outline,
                !tab.busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.toggle_rebase_preview(id, RebaseViewMode::TreeEdit, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .text_size(px(11.)),
        )
    } else if matches!(meta.action, RebaseAction::Pick | RebaseAction::Edit) {
        Some(
            button(
                ("switch-to-reword", id),
                tr!("app.rebase.switch_to_reword"),
                Tone::Outline,
                !tab.busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.enable_rebase_reword(id, entry_index, true, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .text_size(px(11.)),
        )
    } else if meta.action == RebaseAction::Fixup {
        Some(
            button(
                ("switch-to-squash", id),
                tr!("app.rebase.switch_to_squash"),
                Tone::Outline,
                !tab.busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.set_rebase_action(id, entry_index, RebaseAction::Squash, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .text_size(px(11.)),
        )
    } else if meta.action == RebaseAction::Drop {
        Some(
            button(
                ("restore-pick", id),
                tr!("app.rebase.restore_pick"),
                Tone::Outline,
                !tab.busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.set_rebase_action(id, entry_index, RebaseAction::Pick, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .text_size(px(11.)),
        )
    } else {
        None
    };

    let helper_note = if session.mode == RebaseViewMode::ResultPreview && meta.folded_count > 0 {
        Some(tr!("app.rebase.preview_note", count = meta.folded_count))
    } else {
        match meta.action {
            RebaseAction::Edit => Some(tr!("app.rebase.edit_note")),
            RebaseAction::Fixup => Some(tr!("app.rebase.fixup_note")),
            RebaseAction::Drop => Some(tr!("app.rebase.drop_note")),
            _ => None,
        }
    };

    let header = div()
        .flex_shrink_0()
        .px(px(10.))
        .py(px(6.))
        .border_b_1()
        .border_color(rgb(p.border))
        .bg(rgb(p.surface_high))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(rgb(bg))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(fg))
                        .child(short_action_label(meta.action)),
                )
                .when(meta.folded_count > 0, |element| {
                    element.child(
                        div()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded_full()
                            .bg(rgb(p.info_bg))
                            .text_size(px(11.))
                            .text_color(rgb(p.info_fg))
                            .child(tr!("app.rebase.folded_badge", count = meta.folded_count)),
                    )
                }),
        )
        .when_some(quick_button, |element, btn| element.child(btn));

    let body = if editable {
        div().flex_1().min_h_0().p(px(8.)).child(
            Textarea::new(&session.message_editor)
                .text_size(px(13.))
                .h_full(),
        )
    } else {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when_some(helper_note, |element, note_text| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .px(px(12.))
                        .pt(px(8.))
                        .text_size(px(12.))
                        .text_color(rgb(p.text_muted))
                        .child(note_text),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(12.))
                    .text_size(px(14.))
                    .text_color(rgb(
                        if matches!(meta.action, RebaseAction::Drop | RebaseAction::Fixup) {
                            p.text_muted
                        } else {
                            p.text
                        },
                    ))
                    .child(meta.message.clone())
                    .overflow_y_scrollbar(),
            )
    };

    div().size_full().p(px(8.)).child(
        div()
            .size_full()
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(p.border))
            .bg(rgb(p.surface))
            .flex()
            .flex_col()
            .child(header)
            .child(body),
    )
}
