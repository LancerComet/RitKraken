use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_diff::{Diff, DiffHunkSeparator};
use ritkraken_git::Scope;

use crate::i18n::tr;

use super::{DiffLayout, FileReview, staging::attach_controls};
use crate::app::{
    Client, Load, RepoOperation, RepoTab,
    changes::{StageAction, StageTarget, change_mark},
    widgets::{LINE, MUTED, PAPER, TEXT, Tone, WHITE, button, error_text, icon, note},
};

impl DiffLayout {
    pub(in crate::app) fn selector_button(
        self,
        selected: Option<Self>,
        enabled: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        button(
            self.id(),
            tr!(self.label_key()),
            Tone::Outline.selected_when(selected == Some(self)),
            enabled,
            on_click,
        )
    }
}

fn toolbar_toggle(
    id: &'static str,
    label_key: &'static str,
    selected: bool,
    enabled: bool,
    glyph: impl IntoElement,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label: SharedString = tr!(label_key).into();
    button(
        id,
        "",
        Tone::Outline.selected_when(selected),
        enabled,
        on_click,
    )
    .w(px(24.))
    .px(px(0.))
    .child(glyph)
    .aria_label(label.clone())
    .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}

impl Client {
    pub(in crate::app) fn diff_view(
        &self,
        tab: &RepoTab,
        review: &FileReview,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = tab.id;
        let scope = tr!(match &review.file.scope {
            Scope::Worktree => "app.review.worktree",
            Scope::Index => "app.review.index",
            Scope::Commit { .. } => "app.review.commit",
        });
        let mark = change_mark(review.file.change.code);
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
            let diff = Diff::new(state)
                .size_full()
                .header_visible(false)
                .hunk_separator(separator)
                .soft_wrap(tab.review.wrap_lines);
            attach_controls(diff, tab, review, cx).into_any_element()
        } else if review.show_source {
            match &review.source {
                Some(Load::Failed(error)) => error_text(error).into_any_element(),
                _ => note(tr!("app.review.loading_contents")).into_any_element(),
            }
        } else {
            match &review.patch {
                Load::Pending => note(tr!("app.review.loading_diff")).into_any_element(),
                Load::Failed(error) => error_text(error).into_any_element(),
                Load::Ready(_) => note(review.message.as_ref().map_or_else(
                    || tr!("app.review.no_text_changes"),
                    |message| message.text(),
                ))
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
                    .child(div().text_color(rgb(mark.color)).child(mark.glyph))
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
                                action.label(StageTarget::File),
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
                    .child(
                        div().flex().flex_shrink_0().children(
                            [
                                (true, "app.review.file_view"),
                                (false, "app.review.diff_view"),
                            ]
                            .into_iter()
                            .map(|(show, key)| {
                                button(
                                    key,
                                    tr!(key),
                                    Tone::Outline.selected_when(review.show_source == show),
                                    true,
                                    cx.listener(move |this, _, window, cx| {
                                        this.show_source(id, show, window, cx)
                                    }),
                                )
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(229.))
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
                                div()
                                    .flex()
                                    .gap(px(3.))
                                    .flex_shrink_0()
                                    .child(toolbar_toggle(
                                        "ignore-edge-whitespace",
                                        "app.review.ignore_edge_whitespace",
                                        tab.review.ignore_edge_whitespace,
                                        !review.show_source
                                            && !tab.busy
                                            && matches!(review.patch, Load::Ready(_)),
                                        "¶",
                                        cx.listener(move |this, _, window, cx| {
                                            this.toggle_edge_whitespace(id, window, cx);
                                        }),
                                    ))
                                    .child(toolbar_toggle(
                                        "wrap-diff",
                                        "app.preferences.wrap_lines",
                                        tab.review.wrap_lines,
                                        review.state().is_some(),
                                        icon("icons/text-wrap.svg", TEXT),
                                        cx.listener(move |this, _, _, cx| {
                                            if let Some(tab) = this.tab_mut(id) {
                                                tab.review.wrap_lines = !tab.review.wrap_lines;
                                                cx.notify();
                                            }
                                        }),
                                    )),
                            ),
                    ),
            )
            .when(
                !review.show_source
                    && review.compact_context
                    && tab.review.layout != DiffLayout::Hunk,
                |view| {
                    view.child(
                        note(tr!("app.review.available_context"))
                            .py(px(4.))
                            .bg(rgb(PAPER)),
                    )
                },
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }
}
