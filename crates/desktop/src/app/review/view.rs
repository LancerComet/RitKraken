use gpui_kit::{prelude::*, *};
use ritkraken_diff::{Diff, DiffHunkSeparator};
use ritkraken_git::Scope;

use super::{DiffLayout, FileReview};
use crate::app::{
    Client, Load, RepoOperation, RepoTab,
    changes::{StageAction, change_mark},
    widgets::{LINE, MUTED, PAPER, TEXT, Tone, WHITE, button, error_text, icon, note},
};

impl DiffLayout {
    pub(in crate::app) fn selector_button(
        self,
        selected: Option<Self>,
        enabled: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let label = self.label();
        button(
            label,
            label,
            Tone::Outline.selected_when(selected == Some(self)),
            enabled,
            on_click,
        )
    }
}

impl Client {
    pub(in crate::app) fn diff_view(
        &self,
        tab: &RepoTab,
        review: &FileReview,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = tab.id;
        let scope = match &review.file.scope {
            Scope::Worktree => "Working directory",
            Scope::Index => "Staged",
            Scope::Commit { .. } => "Commit",
        };
        let (marker, color) = change_mark(review.file.change.code);
        let stage_action = StageAction::for_scope(&review.file.scope);
        let active_layout = (!review.show_source).then_some(tab.review.layout);
        let can_navigate = !review.show_source && review.viewer.is_some();
        let change = review.file.change.clone();
        let content = if let Some(state) = review.state() {
            let separator = if tab.review.layout == DiffLayout::Hunk {
                DiffHunkSeparator::Metadata
            } else {
                DiffHunkSeparator::Simple
            };
            Diff::new(state)
                .size_full()
                .header_visible(false)
                .hunk_separator(separator)
                .soft_wrap(tab.review.wrap_lines)
                .into_any_element()
        } else if review.show_source {
            match &review.source {
                Some(Load::Failed(error)) => error_text(error).into_any_element(),
                _ => note("Loading file contents…").into_any_element(),
            }
        } else {
            match &review.patch {
                Load::Pending => note("Loading diff…").into_any_element(),
                Load::Failed(error) => error_text(error).into_any_element(),
                Load::Ready(_) => note(
                    review
                        .message
                        .clone()
                        .unwrap_or_else(|| "No textual changes.".into()),
                )
                .into_any_element(),
            }
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(WHITE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .flex_shrink_0()
                    .px(px(9.))
                    .gap(px(7.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(PAPER))
                    .child(div().text_color(rgb(color)).child(marker))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(review.file.change.path.to_string_lossy().into_owned()),
                    )
                    .when_some(stage_action, |row, action| {
                        row.child(
                            button(
                                ("review-file-action", id),
                                format!("{} File", action.verb()),
                                action.tone(),
                                !tab.busy,
                                cx.listener(move |this, _, window, cx| {
                                    let edit =
                                        RepoOperation::StageFiles(action, Some(change.clone()));
                                    this.update_repo(id, Some(edit), window, cx);
                                }),
                            )
                            .flex_shrink_0(),
                        )
                    })
                    .child(
                        button(
                            ("close-diff", id),
                            "",
                            Tone::Plain,
                            true,
                            cx.listener(|this, _, window, cx| this.close_diff(window, cx)),
                        )
                        .w(px(27.))
                        .child(icon("icons/x.svg", TEXT)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(39.))
                    .flex_shrink_0()
                    .gap(px(6.))
                    .px(px(8.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div().flex_1().min_w_0().child(
                            div()
                                .truncate()
                                .px(px(4.))
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(scope),
                        ),
                    )
                    .child(div().flex().flex_shrink_0().children(
                        [(true, "File View"), (false, "Diff View")].into_iter().map(
                            |(show, label)| {
                                button(
                                    label,
                                    label,
                                    Tone::Outline.selected_when(review.show_source == show),
                                    true,
                                    cx.listener(move |this, _, window, cx| {
                                        this.show_source(id, show, window, cx)
                                    }),
                                )
                            },
                        ),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(202.))
                            .flex()
                            .justify_end()
                            .items_center()
                            .gap(px(5.))
                            .child(
                                div().flex().flex_shrink_0().children(
                                    [(false, "↑"), (true, "↓")].into_iter().map(
                                        |(forward, glyph)| {
                                            button(
                                                glyph,
                                                glyph,
                                                Tone::Outline,
                                                can_navigate,
                                                cx.listener(move |this, _, _, cx| {
                                                    this.move_diff_change(id, forward, cx)
                                                }),
                                            )
                                            .w(px(24.))
                                            .px(px(0.))
                                        },
                                    ),
                                ),
                            )
                            .child(div().flex().flex_shrink_0().children(
                                DiffLayout::ALL.into_iter().map(|layout| {
                                    layout
                                        .selector_button(
                                            active_layout,
                                            review.viewer.is_some(),
                                            cx.listener(move |this, _, _, cx| {
                                                this.set_diff_layout(id, layout, cx)
                                            }),
                                        )
                                        .px(px(5.))
                                        .text_size(px(11.))
                                }),
                            ))
                            .child(
                                button(
                                    "wrap-diff",
                                    "¶",
                                    Tone::Outline.selected_when(tab.review.wrap_lines),
                                    review.state().is_some(),
                                    cx.listener(move |this, _, _, cx| {
                                        if let Some(tab) = this.tab_mut(id) {
                                            tab.review.wrap_lines = !tab.review.wrap_lines;
                                            cx.notify();
                                        }
                                    }),
                                )
                                .w(px(24.))
                                .px(px(0.)),
                            ),
                    ),
            )
            .when(
                !review.show_source
                    && review.compact_context
                    && tab.review.layout != DiffLayout::Hunk,
                |view| {
                    view.child(
                        note("Showing the available patch context.")
                            .py(px(4.))
                            .bg(rgb(PAPER)),
                    )
                },
            )
            .when(!review.show_source && !review.hunks.is_empty(), |view| {
                view.child(self.hunk_actions(tab, review, cx))
            })
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }

    fn hunk_actions(&self, tab: &RepoTab, review: &FileReview, cx: &mut Context<Self>) -> Div {
        let (Load::Ready(patch), Some(action)) =
            (&review.patch, StageAction::for_scope(&review.file.scope))
        else {
            return div();
        };
        let id = tab.id;
        div()
            .h(px(31.))
            .flex_shrink_0()
            .min_w_0()
            .border_b_1()
            .border_color(rgb(LINE))
            .bg(rgb(PAPER))
            .child(
                div()
                    .id(("hunks", id))
                    .size_full()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px(px(7.))
                    .overflow_x_scroll()
                    .children(review.hunks.iter().enumerate().map(|(index, header)| {
                        let patch = patch.clone();
                        let file = review.file.clone();
                        let position = header.split("@@").nth(1).unwrap_or(header).trim();
                        button(
                            ("hunk-action", index),
                            format!("{} hunk {}  ({position})", action.verb(), index + 1),
                            action.tone(),
                            !tab.busy,
                            cx.listener(move |this, _, window, cx| {
                                this.update_repo(
                                    id,
                                    Some(RepoOperation::Hunk(file.clone(), patch.clone(), index)),
                                    window,
                                    cx,
                                );
                            }),
                        )
                        .flex_shrink_0()
                        .h(px(22.))
                    })),
            )
    }
}
