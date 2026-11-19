use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_diff::{Diff, DiffHunkSeparator, DiffSearchLabels, DiffState, DiffTypography};
use ritkraken_git::Scope;

use crate::i18n::tr;

use super::{DiffLayout, FileReview, staging::attach_controls};
use crate::app::{
    Client, Load, RepoOperation, RepoTab,
    changes::{StageAction, StageTarget, change_mark},
    theme::{Palette, palette},
    widgets::{Tone, bento_card, button, error_text, icon, note},
};

impl DiffLayout {
    pub(in crate::app) fn viewer(
        self,
        state: &Entity<DiffState>,
        wrap_lines: bool,
        typography: DiffTypography,
    ) -> Diff {
        let separator = if self == Self::Hunk {
            DiffHunkSeparator::Metadata
        } else {
            DiffHunkSeparator::Simple
        };
        Diff::new(state)
            .size_full()
            .header_visible(false)
            .hunk_separator(separator)
            .soft_wrap(wrap_lines)
            .typography(typography)
            .search_labels(DiffSearchLabels {
                placeholder: tr!("app.review.find_placeholder").into(),
                match_case: tr!("app.review.find_case").into(),
                previous: tr!("app.review.find_previous").into(),
                next: tr!("app.review.find_next").into(),
                close: tr!("app.review.find_close").into(),
            })
    }

    pub(in crate::app) fn selector_button(
        self,
        selected: Option<Self>,
        enabled: bool,
        cx: &App,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let active = selected == Some(self);
        button(
            self.id(),
            tr!(self.label_key()),
            Tone::Plain.selected_when(active),
            enabled,
            cx,
            on_click,
        )
        .rounded_full()
        .h(px(22.))
    }
}

fn toolbar_toggle(
    id: &'static str,
    label_key: &'static str,
    selected: bool,
    enabled: bool,
    glyph: impl IntoElement,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label: SharedString = tr!(label_key).into();
    button(
        id,
        "",
        Tone::Outline.selected_when(selected),
        enabled,
        cx,
        on_click,
    )
    .size(px(26.))
    .px(px(0.))
    .rounded_full()
    .child(glyph)
    .aria_label(label.clone())
    .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}

fn pill_group(p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .flex_shrink_0()
        .rounded_full()
        .bg(rgb(p.surface_dim))
        .border_1()
        .border_color(rgb(p.border))
}

impl Client {
    pub(in crate::app) fn diff_view(
        &self,
        tab: &RepoTab,
        review: &FileReview,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let id = tab.id;
        let sequence = tab
            .history
            .data
            .snapshot
            .sequence
            .filter(|kind| *kind != ritkraken_git::SequenceKind::Other)
            .map(|kind| crate::app::history::rebase::sequence_control_bar(tab, kind, cx));
        if let Some(conflict) = &review.conflict {
            return bento_card(p)
                .size_full()
                .flex()
                .flex_col()
                .children(sequence)
                .child(div().flex_1().min_h_0().child(conflict.clone()));
        }
        let scope = tr!(match &review.file.scope {
            Scope::Worktree => "app.review.worktree",
            Scope::Index => "app.review.index",
            Scope::Commit { .. } => "app.review.commit",
        });
        let mark = change_mark(review.file.change.code, p);
        let stage_action =
            StageAction::for_scope(&review.file.scope).filter(|_| review.file.change.code != b'U');
        let active_layout = (!review.show_source).then_some(tab.review.layout);
        let can_navigate = !review.show_source && review.viewer.is_some();
        let change = review.file.change.clone();
        let content = if let Some(state) = review.state() {
            let diff = tab.review.layout.viewer(
                state,
                tab.review.wrap_lines,
                self.saved.preferences.editor.diff_typography(),
            );
            attach_controls(diff, tab, review, cx).into_any_element()
        } else if review.show_source {
            match &review.source {
                Some(Load::Failed(error)) => error_text(error, cx).into_any_element(),
                _ => note(tr!("app.review.loading_contents"), cx).into_any_element(),
            }
        } else {
            match &review.patch {
                Load::Pending => note(tr!("app.review.loading_diff"), cx).into_any_element(),
                Load::Failed(error) => error_text(error, cx).into_any_element(),
                Load::Ready(_) => note(
                    review.message.as_ref().map_or_else(
                        || tr!("app.review.no_text_changes"),
                        |message| message.text(),
                    ),
                    cx,
                )
                .into_any_element(),
            }
        };

        bento_card(p)
            .size_full()
            .flex()
            .flex_col()
            .children(sequence)
            .text_color(rgb(p.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px(px(10.))
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .bg(rgb(p.surface_high))
                    .child(div().text_color(rgb(mark.color)).child(mark.glyph))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(review.file.change.path.to_string_lossy().into_owned()),
                    )
                    .when_some(stage_action, |row, action| {
                        row.child(
                            button(
                                ("review-file-action", id),
                                action.label(StageTarget::File),
                                action.tone(),
                                !tab.busy,
                                cx,
                                cx.listener(move |this, _, window, cx| {
                                    let edit = RepoOperation::StageFiles(
                                        action,
                                        Some(vec![change.clone()]),
                                    );
                                    this.update_repo(id, Some(edit), window, cx);
                                }),
                            )
                            .rounded_full()
                            .px(px(12.))
                            .flex_shrink_0(),
                        )
                    })
                    .child(
                        button(
                            ("close-diff", id),
                            "",
                            Tone::Plain,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.close_diff(window, cx)),
                        )
                        .size(px(26.))
                        .px(px(0.))
                        .rounded_full()
                        .child(icon("icons/x.svg", p.text)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.))
                    .flex_shrink_0()
                    .gap(px(8.))
                    .px(px(10.))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .bg(rgb(p.canvas))
                    .child(
                        div().flex_1().min_w_0().child(
                            div()
                                .truncate()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded_full()
                                .bg(rgb(p.surface_dim))
                                .text_size(px(11.))
                                .text_color(rgb(p.text_muted))
                                .child(scope),
                        ),
                    )
                    .child(
                        pill_group(p).children(
                            [
                                (true, "app.review.file_view"),
                                (false, "app.review.diff_view"),
                            ]
                            .into_iter()
                            .map(|(show, key)| {
                                button(
                                    key,
                                    tr!(key),
                                    Tone::Plain.selected_when(review.show_source == show),
                                    true,
                                    cx,
                                    cx.listener(move |this, _, window, cx| {
                                        this.show_source(id, show, window, cx)
                                    }),
                                )
                                .rounded_full()
                                .h(px(22.))
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(240.))
                            .flex()
                            .justify_end()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                pill_group(p).children(
                                    [(false, "↑"), (true, "↓")].into_iter().map(
                                        |(forward, glyph)| {
                                            button(
                                                glyph,
                                                glyph,
                                                Tone::Plain,
                                                can_navigate,
                                                cx,
                                                cx.listener(move |this, _, _, cx| {
                                                    this.move_diff_change(id, forward, cx)
                                                }),
                                            )
                                            .rounded_full()
                                            .size(px(22.))
                                            .px(px(0.))
                                        },
                                    ),
                                ),
                            )
                            .child(pill_group(p).children(DiffLayout::ALL.into_iter().map(
                                |layout| {
                                    layout
                                        .selector_button(
                                            active_layout,
                                            review.viewer.is_some(),
                                            cx,
                                            cx.listener(move |this, _, _, cx| {
                                                this.set_diff_layout(id, layout, cx)
                                            }),
                                        )
                                        .px(px(7.))
                                        .text_size(px(11.))
                                },
                            )))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(4.))
                                    .flex_shrink_0()
                                    .child(toolbar_toggle(
                                        "find-in-file",
                                        "app.review.find",
                                        review
                                            .state()
                                            .is_some_and(|state| state.read(cx).is_search_open()),
                                        review.state().is_some(),
                                        icon("icons/search.svg", p.text),
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            if let Some(state) = this
                                                .tab(id)
                                                .and_then(|tab| tab.review.active.as_ref())
                                                .and_then(FileReview::state)
                                                .cloned()
                                            {
                                                state.update(cx, |state, cx| {
                                                    state.open_search(window, cx)
                                                });
                                            }
                                        }),
                                    ))
                                    .child(toolbar_toggle(
                                        "ignore-edge-whitespace",
                                        "app.review.ignore_edge_whitespace",
                                        tab.review.ignore_edge_whitespace,
                                        !review.show_source
                                            && !tab.busy
                                            && matches!(review.patch, Load::Ready(_)),
                                        "¶",
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            this.toggle_edge_whitespace(id, window, cx);
                                        }),
                                    ))
                                    .child(toolbar_toggle(
                                        "wrap-diff",
                                        "app.preferences.wrap_lines",
                                        tab.review.wrap_lines,
                                        review.state().is_some(),
                                        icon("icons/text-wrap.svg", p.text),
                                        cx,
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
                        note(tr!("app.review.available_context"), cx)
                            .py(px(4.))
                            .bg(rgb(p.surface_dim)),
                    )
                },
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }
}
