use gpui_kit::{
    component::{scroll::ScrollableElement, tooltip::Tooltip},
    prelude::*,
    *,
};
use ritkraken_git::{Commit, short_id};

use crate::{
    app::{
        Load,
        theme::palette,
        widgets::{Tone, button, error_text, note},
    },
    i18n::tr,
};

use super::{PickerSide, RefCompare, Selection};

/// `n` or `n+` when the git read hit the fetch limit.
fn count(count: usize, limited: bool) -> String {
    if limited {
        format!("{count}+")
    } else {
        count.to_string()
    }
}

impl Render for RefCompare {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(p.surface))
            .child(self.header(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.sidebar(cx))
                    .child(div().flex_1().min_w_0().h_full().child(self.content(cx))),
            )
    }
}

impl RefCompare {
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(p.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(40.))
                    .px(px(12.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(tr!("app.compare.base")),
                    )
                    .child(self.ref_picker(PickerSide::Base, cx))
                    .child(
                        button(
                            "compare-swap",
                            "⇄",
                            Tone::Outline,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.swap(window, cx)),
                        )
                        .tooltip(|window, cx| {
                            Tooltip::new(tr!("app.compare.swap")).build(window, cx)
                        }),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(tr!("app.compare.with")),
                    )
                    .child(self.ref_picker(PickerSide::Compare, cx)),
            )
            .child(self.summary(cx))
    }

    fn summary(&self, cx: &App) -> impl IntoElement {
        let p = palette(cx);
        let text = match &self.result {
            Load::Ready(comparison) => {
                let compare_count =
                    count(comparison.only_compare.len(), comparison.compare_limited);
                let base_count = count(comparison.only_base.len(), comparison.base_limited);
                match &comparison.merge_base {
                    Some(merge_base) => tr!(
                        "app.compare.summary",
                        compare_count = compare_count,
                        compare = self.compare.label.clone(),
                        base_count = base_count,
                        base = self.base.label.clone(),
                        merge_base = short_id(merge_base)
                    ),
                    None => tr!(
                        "app.compare.summary_unrelated",
                        compare_count = compare_count,
                        compare = self.compare.label.clone(),
                        base_count = base_count,
                        base = self.base.label.clone()
                    ),
                }
            }
            Load::Pending => tr!("app.compare.loading"),
            Load::Failed(message) => message.text(),
        };
        div()
            .px(px(12.))
            .pb(px(9.))
            .text_size(px(11.))
            .text_color(rgb(p.text_muted))
            .truncate()
            .child(text)
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let mut column = div()
            .w(px(340.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(p.border));
        match &self.result {
            Load::Pending => column = column.child(note(tr!("app.compare.loading"), cx)),
            Load::Failed(_) => {}
            Load::Ready(comparison) => {
                let selected = self.selection == Selection::All;
                column = column
                    .child(
                        div()
                            .id("compare-all-changes")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .px(px(10.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(rgb(p.border))
                            .when(selected, |d| d.bg(rgb(p.border)))
                            .hover(|d| d.bg(rgb(p.surface_dim)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(rgb(p.text))
                                    .child(tr!("app.compare.all_changes")),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.show_all(cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .child(self.section(
                                &comparison.only_compare,
                                tr!(
                                    "app.compare.only_in",
                                    name = self.compare.label.clone(),
                                    count = count(
                                        comparison.only_compare.len(),
                                        comparison.compare_limited
                                    )
                                ),
                                cx,
                            ))
                            .child(self.section(
                                &comparison.only_base,
                                tr!(
                                    "app.compare.only_in",
                                    name = self.base.label.clone(),
                                    count =
                                        count(comparison.only_base.len(), comparison.base_limited)
                                ),
                                cx,
                            )),
                    );
            }
        }
        column
    }

    fn section(
        &self,
        commits: &[Commit],
        title: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        let mut section = div().flex().flex_col().child(
            div()
                .px(px(10.))
                .pt(px(8.))
                .pb(px(4.))
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(p.text_muted))
                .truncate()
                .child(title),
        );
        for commit in commits {
            section = section.child(self.commit_row(commit, cx));
        }
        section
    }

    fn commit_row(&self, commit: &Commit, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let id = commit.id.clone();
        let selected = self.selection == Selection::Commit(commit.id.clone());
        div()
            .id(format!("compare-commit-{}", commit.id))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(10.))
            .py(px(6.))
            .when(selected, |d| d.bg(rgb(p.border)))
            .hover(|d| d.bg(rgb(p.surface_dim)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(short_id(&commit.id).to_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(rgb(p.text))
                            .child(commit.subject.clone()),
                    ),
            )
            .child(
                div()
                    .pl(px(6.))
                    .text_size(px(11.))
                    .text_color(rgb(p.text_muted))
                    .truncate()
                    .child(format!("{} · {}", commit.author, commit.date)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_commit(id.clone(), window, cx);
            }))
    }

    fn content(&self, cx: &App) -> AnyElement {
        let view = match &self.selection {
            Selection::All => &self.all_view,
            Selection::Commit(_) => &self.commit_view,
        };
        match &self.result {
            Load::Pending => note(tr!("app.compare.loading"), cx).into_any_element(),
            Load::Failed(message) => error_text(message, cx).into_any_element(),
            Load::Ready(_) => match view {
                Load::Pending => note(tr!("app.compare.loading"), cx).into_any_element(),
                Load::Failed(message) => error_text(message, cx).into_any_element(),
                Load::Ready(state) => self
                    .layout
                    .viewer(state, self.wrap_lines, self.typography.clone())
                    .header_visible(true)
                    .into_any_element(),
            },
        }
    }
}
