use std::{ops::Range, path::Path, time::Duration};

use gpui_kit::{prelude::*, *};

use super::{Entry, Level, entries, generation, log_file};
use crate::{
    app::{
        Client, Escape,
        modals::{ContentModal, ModalHandle},
        theme::{Palette, palette},
        widgets::{Tone, button},
    },
    i18n::tr,
};

/// New entries usually arrive in bursts of Git calls; checking twice a second
/// keeps an open viewer current without redrawing for every call.
const REFRESH_INTERVAL: Duration = Duration::from_millis(500);
const ROW_HEIGHT: Pixels = px(24.);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Errors,
    Warnings,
    Git,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Errors, Self::Warnings, Self::Git];

    fn label(self) -> &'static str {
        match self {
            Self::All => "app.logs.all",
            Self::Errors => "app.logs.errors",
            Self::Warnings => "app.logs.warnings",
            Self::Git => "app.logs.git",
        }
    }

    fn accepts(self, entry: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Errors => entry.level == Level::Error,
            Self::Warnings => entry.level == Level::Warning,
            Self::Git => entry.level == Level::Git,
        }
    }
}

struct LogViewer {
    modal: ModalHandle,
    filter: Filter,
    all: Vec<Entry>,
    visible: Vec<Entry>,
    generation: u64,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    _refresh: Task<()>,
}

impl Client {
    pub(in crate::app) fn open_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_is_present() {
            return;
        }
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let refresh = cx.spawn(async move |view, cx| {
                loop {
                    cx.background_executor().timer(REFRESH_INTERVAL).await;
                    if view
                        .update(cx, |this: &mut LogViewer, cx| this.reload(cx))
                        .is_err()
                    {
                        return;
                    }
                }
            });
            let mut viewer = LogViewer {
                modal: handle,
                filter: Filter::All,
                all: Vec::new(),
                visible: Vec::new(),
                generation: u64::MAX,
                selected: None,
                scroll: UniformListScrollHandle::default(),
                _refresh: refresh,
            };
            viewer.reload(cx);
            viewer
        });
        Self::present_content_modal(
            ContentModal::new(
                modal,
                tr!("app.logs.title"),
                view,
                size(px(1000.), px(700.)),
                false,
            ),
            window,
            cx,
        );
    }
}

impl LogViewer {
    fn reload(&mut self, cx: &mut Context<Self>) {
        let current = generation();
        if current == self.generation {
            return;
        }
        // Keep the same entry selected while newer entries are added above it.
        let selected = self
            .selected
            .and_then(|index| self.visible.get(index))
            .map(|entry| (entry.time, entry.summary.clone()));
        self.generation = current;
        self.all = entries();
        self.apply_filter();
        self.selected = selected.and_then(|(time, summary)| {
            self.visible
                .iter()
                .position(|entry| entry.time == time && entry.summary == summary)
        });
        cx.notify();
    }

    fn apply_filter(&mut self) {
        let filter = self.filter;
        self.visible = self
            .all
            .iter()
            .filter(|entry| filter.accepts(entry))
            .cloned()
            .collect();
    }

    fn choose(&mut self, filter: Filter, cx: &mut Context<Self>) {
        if self.filter != filter {
            self.filter = filter;
            self.selected = None;
            self.apply_filter();
            cx.notify();
        }
    }

    fn row(&self, index: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let entry = &self.visible[index];
        let (tag, color) = level_style(entry, p);
        let selected = self.selected == Some(index);
        div()
            .id(("log-entry", index))
            .h(ROW_HEIGHT)
            .w_full()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(4.))
            .text_size(px(12.))
            .cursor_pointer()
            .when(selected, |row| row.bg(rgb(p.accent_soft)))
            .when(!selected, |row| row.hover(|row| row.bg(rgb(p.hover))))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(p.text_muted))
                    .child(entry.time.format("%H:%M:%S").to_string()),
            )
            .child(
                div()
                    .w(px(52.))
                    .flex_shrink_0()
                    .text_color(rgb(color))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr!(tag)),
            )
            .when_some(entry.source.clone(), |row, source| {
                row.child(
                    div()
                        .max_w(px(140.))
                        .flex_shrink_0()
                        .truncate()
                        .text_color(rgb(p.text_muted))
                        .child(source),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.summary.clone()),
            )
            .when_some(entry.git.as_ref(), |row, git| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(p.text_muted))
                        .child(format!("{} ms", git.duration.as_millis())),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = Some(index);
                cx.notify();
            }))
            .into_any_element()
    }

    fn detail(&self, p: Palette) -> AnyElement {
        let Some(entry) = self.selected.and_then(|index| self.visible.get(index)) else {
            return div()
                .text_color(rgb(p.text_muted))
                .child(tr!("app.logs.select"))
                .into_any_element();
        };
        div()
            .id("log-detail-text")
            .size_full()
            .overflow_y_scroll()
            .font_family("monospace")
            .text_size(px(12.))
            .whitespace_normal()
            .child(entry.text())
            .into_any_element()
    }
}

fn level_style(entry: &Entry, p: Palette) -> (&'static str, u32) {
    match entry.level {
        Level::Error => ("app.logs.error", p.danger),
        Level::Warning => ("app.logs.warning", p.warning),
        Level::Git if entry.is_problem() => ("app.logs.git", p.danger),
        Level::Git => ("app.logs.git", p.text_muted),
    }
}

impl Render for LogViewer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let filters = Filter::ALL
            .into_iter()
            .fold(div().flex().gap(px(6.)), |bar, filter| {
                let count = self
                    .all
                    .iter()
                    .filter(|entry| filter.accepts(entry))
                    .count();
                bar.child(button(
                    SharedString::from(format!("log-filter-{}", filter.label())),
                    format!("{} {count}", tr!(filter.label())),
                    Tone::Outline.selected_when(self.filter == filter),
                    true,
                    cx,
                    cx.listener(move |this, _, _, cx| this.choose(filter, cx)),
                ))
            });
        let selected_text = self
            .selected
            .and_then(|index| self.visible.get(index))
            .map(Entry::text);
        let list = if self.visible.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(p.text_muted))
                .child(tr!("app.logs.empty"))
                .into_any_element()
        } else {
            uniform_list(
                "log-entries",
                self.visible.len(),
                cx.processor(move |this, range: Range<usize>, _, cx| {
                    range.map(|index| this.row(index, p, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(filters)
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(button(
                                "copy-log-entry",
                                tr!("app.logs.copy"),
                                Tone::Outline,
                                selected_text.is_some(),
                                cx,
                                move |_, _, cx| {
                                    if let Some(text) = &selected_text {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            text.clone(),
                                        ));
                                    }
                                },
                            ))
                            .child(button(
                                "open-log-folder",
                                tr!("app.logs.open_folder"),
                                Tone::Outline,
                                log_file().is_some(),
                                cx,
                                |_, _, cx| {
                                    if let Some(folder) = log_file()
                                        .and_then(|file| file.parent().map(Path::to_path_buf))
                                    {
                                        let _ = std::fs::create_dir_all(&folder);
                                        cx.open_with_system(&folder);
                                    }
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p(px(4.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(p.border))
                    .child(list),
            )
            .child(
                div()
                    .h(px(150.))
                    .flex_shrink_0()
                    .p(px(10.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(p.border))
                    .bg(rgb(p.surface_dim))
                    .child(self.detail(p)),
            )
    }
}
