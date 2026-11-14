use gpui_kit::{prelude::*, *};
use ritkraken_git::{self as git, CommitOperation, SequenceKind};

use super::{RebaseSession, RebaseViewMode};
use crate::{
    app::{
        Client, RepoTab,
        theme::palette,
        widgets::{Tone, button, icon},
    },
    i18n::tr,
};

pub(in crate::app) fn rebase_planning_bar(
    tab: &RepoTab,
    session: &RebaseSession,
    cx: &mut Context<Client>,
) -> Div {
    let p = palette(cx);
    let id = tab.id;
    let busy = tab.busy;
    let branch = session
        .context
        .branch
        .as_deref()
        .unwrap_or("HEAD")
        .to_owned();
    let base_short = git::short_id(&session.plan.base);
    let valid = session.projected.validation_error.is_none();

    let summary = if let Some(error_key) = session.projected.validation_error {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(11.5))
            .text_color(rgb(p.danger))
            .child(icon("icons/circle-alert.svg", p.danger).size(px(13.)))
            .child(tr!(error_key))
    } else {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(rgb(p.text_muted))
            .child(tr!(
                "app.rebase.summary",
                branch = branch,
                base = base_short,
                kept = session.projected.kept_count,
                folded = session.projected.folded_count,
                dropped = session.projected.dropped_count
            ))
    };

    let edit_mode = session.mode == RebaseViewMode::TreeEdit;
    let mode_toggle = div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .rounded_full()
        .bg(rgb(p.surface_dim))
        .border_1()
        .border_color(rgb(p.border))
        .child(
            button(
                ("rebase-mode-edit", id),
                tr!("app.rebase.mode_edit"),
                Tone::Plain.selected_when(edit_mode),
                !busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.toggle_rebase_preview(id, RebaseViewMode::TreeEdit, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .px(px(10.))
            .text_size(px(11.)),
        )
        .child(
            button(
                ("rebase-mode-preview", id),
                tr!("app.rebase.mode_preview"),
                Tone::Plain.selected_when(!edit_mode),
                !busy,
                cx,
                cx.listener(move |this, _, window, cx| {
                    this.toggle_rebase_preview(id, RebaseViewMode::ResultPreview, window, cx);
                }),
            )
            .rounded_full()
            .h(px(22.))
            .px(px(10.))
            .text_size(px(11.)),
        );

    div()
        .w_full()
        .flex_shrink_0()
        .bg(rgb(p.surface_high))
        .border_b_1()
        .border_color(rgb(p.border))
        .px(px(12.))
        .py(px(7.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_shrink_0()
                        .px(px(8.))
                        .py(px(2.))
                        .rounded_full()
                        .bg(rgb(p.accent_soft))
                        .border_1()
                        .border_color(rgb(p.accent_soft_border))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(p.accent_soft_fg))
                        .child(tr!("app.rebase.badge")),
                )
                .child(div().flex_1().min_w_0().truncate().child(summary)),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(mode_toggle)
                .child(
                    button(
                        ("rebase-reset", id),
                        tr!("app.rebase.reset"),
                        Tone::Plain,
                        !busy,
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            this.reset_rebase(id, window, cx);
                        }),
                    )
                    .rounded_full()
                    .h(px(26.))
                    .px(px(10.))
                    .text_size(px(11.5)),
                )
                .child(
                    button(
                        ("rebase-cancel", id),
                        tr!("app.common.cancel"),
                        Tone::Outline,
                        !busy,
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            this.cancel_rebase(id, window, cx);
                        }),
                    )
                    .rounded_full()
                    .h(px(26.))
                    .px(px(12.))
                    .text_size(px(11.5)),
                )
                .child(
                    button(
                        ("rebase-start", id),
                        tr!("app.commit_menu.start_rebase"),
                        Tone::Green,
                        !busy && valid,
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            this.submit_rebase(id, window, cx);
                        }),
                    )
                    .rounded_full()
                    .h(px(26.))
                    .px(px(12.))
                    .text_size(px(11.5)),
                ),
        )
}

pub(in crate::app) fn sequence_control_bar(
    tab: &RepoTab,
    kind: SequenceKind,
    cx: &mut Context<Client>,
) -> Div {
    let p = palette(cx);
    let id = tab.id;
    let busy = tab.busy;
    let snapshot = &tab.history.data.snapshot;
    let conflicted = snapshot.status.has_conflicts();
    let editing = snapshot.is_editing_rebase_commit();
    let has_head = snapshot
        .head
        .as_deref()
        .is_some_and(|head| tab.history.data.position.contains_key(head));

    let kind_label = tr!(match kind {
        SequenceKind::Rebase => "app.rebase.seq_rebase",
        SequenceKind::CherryPick => "app.rebase.seq_cherry_pick",
        SequenceKind::Revert => "app.rebase.seq_revert",
        SequenceKind::Merge => "app.rebase.seq_merge",
        SequenceKind::Other => "app.rebase.seq_other",
    });

    let status_note = if conflicted {
        tr!("app.rebase.status_conflicted")
    } else if editing {
        tr!("app.rebase.status_editing")
    } else {
        tr!("app.rebase.status_paused")
    };

    let progress_badge = snapshot.sequence_progress().and_then(|progress| {
        let label = match (progress.current_step, progress.total_steps) {
            (Some(current), Some(total)) => format!("{current} / {total}"),
            (Some(current), None) => format!("{current}"),
            _ => return None,
        };
        Some(
            div()
                .flex_shrink_0()
                .px(px(7.))
                .py(px(1.))
                .rounded_full()
                .bg(rgb(p.surface_dim))
                .border_1()
                .border_color(rgb(p.border))
                .text_size(px(11.))
                .text_color(rgb(p.text))
                .child(label),
        )
    });

    let supports_skip = kind.can_skip();

    let accent_color = if conflicted { p.danger } else { p.warning };
    let badge_bg = if conflicted {
        p.danger_bg
    } else {
        p.warning_bg
    };

    div()
        .w_full()
        .flex_shrink_0()
        .bg(rgb(p.surface_high))
        .border_b_1()
        .border_color(rgb(p.border))
        .px(px(12.))
        .py(px(7.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_shrink_0()
                        .px(px(8.))
                        .py(px(2.))
                        .rounded_full()
                        .bg(rgb(badge_bg))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(accent_color))
                        .child(kind_label),
                )
                .when_some(progress_badge, |element, badge| element.child(badge))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(rgb(if conflicted { p.danger } else { p.text_muted }))
                        .child(status_note),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .when(editing, |element| {
                    element.child(
                        button(
                            ("seq-amend", id),
                            tr!("app.rebase.amend_commit"),
                            Tone::Selected,
                            !busy && !conflicted && has_head,
                            cx,
                            cx.listener(move |this, _, window, cx| {
                                this.amend_paused_rebase(id, window, cx);
                            }),
                        )
                        .rounded_full()
                        .h(px(26.))
                        .px(px(10.))
                        .text_size(px(11.5)),
                    )
                })
                .child(
                    button(
                        ("seq-abort", id),
                        tr!("app.rebase.seq_abort"),
                        Tone::Red,
                        !busy && has_head,
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            this.run_sequence_action(id, CommitOperation::Abort, window, cx);
                        }),
                    )
                    .rounded_full()
                    .h(px(26.))
                    .px(px(10.))
                    .text_size(px(11.5)),
                )
                .when(supports_skip, |element| {
                    element.child(
                        button(
                            ("seq-skip", id),
                            tr!("app.rebase.seq_skip"),
                            Tone::Warning,
                            !busy && has_head,
                            cx,
                            cx.listener(move |this, _, window, cx| {
                                this.run_sequence_action(id, CommitOperation::Skip, window, cx);
                            }),
                        )
                        .rounded_full()
                        .h(px(26.))
                        .px(px(10.))
                        .text_size(px(11.5)),
                    )
                })
                .child(
                    button(
                        ("seq-continue", id),
                        tr!("app.rebase.seq_continue"),
                        Tone::Green,
                        !busy && !conflicted && has_head,
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            this.run_sequence_action(id, CommitOperation::Continue, window, cx);
                        }),
                    )
                    .rounded_full()
                    .h(px(26.))
                    .px(px(12.))
                    .text_size(px(11.5)),
                ),
        )
}
