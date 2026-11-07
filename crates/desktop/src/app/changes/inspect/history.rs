use std::sync::Arc;

use gpui_kit::{component::scroll::ScrollableElement, prelude::*, *};
use ritkraken_git::{self as git, FileHistory};

use super::{FileInspection, Load};
use crate::{
    app::{
        review::DiffLayout,
        widgets::{HOVER, LINE, MUTED, SELECT, Tone, button, error_text, note, table::Column},
    },
    i18n::tr,
};

const COLUMNS: [Column; 4] = [
    Column {
        label: "app.history.sha",
        width: Some(px(80.)),
        minimum: px(64.),
    },
    Column {
        label: "app.history.message",
        width: None,
        minimum: px(160.),
    },
    Column {
        label: "app.history.author",
        width: Some(px(160.)),
        minimum: px(90.),
    },
    Column {
        label: "app.history.date_time",
        width: Some(px(180.)),
        minimum: px(110.),
    },
];

fn cell(value: impl Into<SharedString>) -> Div {
    div()
        .size_full()
        .min_w_0()
        .px(px(10.))
        .flex()
        .items_center()
        .child(div().w_full().truncate().child(value.into()))
}

impl FileInspection {
    pub(super) fn history_view(&self, history: &Arc<FileHistory>, cx: &mut Context<Self>) -> Div {
        if history.commits.is_empty() {
            return div().child(note(tr!("app.file_menu.no_history")));
        }
        let rows = history.clone();
        let selected = self.selected.clone();
        let list = uniform_list(
            "file-history",
            rows.commits.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                let widths = this.history_columns.widths(&COLUMNS, cx);
                range
                    .map(|index| {
                        let commit = &rows.commits[index].commit;
                        div()
                            .id(("file-history-row", index))
                            .w_full()
                            .h(px(32.))
                            .bg(rgb(if selected.as_deref() == Some(commit.id.as_str()) {
                                SELECT
                            } else {
                                0xffffff
                            }))
                            .hover(|style| style.bg(rgb(HOVER)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_revision(index, window, cx);
                            }))
                            .child(
                                div().size_full().flex().children(
                                    [
                                        git::short_id(&commit.id).to_owned(),
                                        commit.subject.clone(),
                                        commit.author.clone(),
                                        commit.date.clone(),
                                    ]
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, value)| {
                                        widths
                                            .cell(index, cell(value))
                                            .when(index != 1, |cell| cell.text_color(rgb(MUTED)))
                                    }),
                                ),
                            )
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        let preview = match &self.preview {
            Some(Load::Ready(viewer)) => DiffLayout::Hunk
                .viewer(viewer, false, self.typography.clone())
                .into_any_element(),
            Some(Load::Failed(error)) => error_text(error).into_any_element(),
            _ => note(tr!("app.review.loading_diff")).into_any_element(),
        };
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .w_full()
                    .h(relative(0.4))
                    .min_h(px(90.))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w_full()
                            .min_w(self.history_columns.minimum_width(&COLUMNS, cx))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(self.history_columns.header(
                                "file-history-columns",
                                &COLUMNS,
                                px(10.),
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .flex_1()
                                    .min_h_0()
                                    .relative()
                                    .child(list)
                                    .vertical_scrollbar(&self.scroll),
                            ),
                    )
                    .overflow_x_scrollbar(),
            )
            .when(history.has_more, |body| {
                body.child(button(
                    "more-file-history",
                    tr!("app.file_menu.load_more"),
                    Tone::Outline,
                    !self.loading,
                    cx.listener(|this, _, window, cx| {
                        this.limit = this.limit.saturating_add(500);
                        this.load(window, cx);
                    }),
                ))
            })
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(preview),
            )
    }
}
