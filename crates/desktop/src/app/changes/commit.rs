use gpui_kit::{
    component::{
        Sizable,
        resizable::{resizable_panel, v_resizable},
        scroll::ScrollableElement,
    },
    prelude::*,
    *,
};
use ritkraken_git::{self as git, Change, Scope};

use crate::i18n::tr;

use super::{change_count, change_mark};
use crate::app::{
    Client, Load, RepoTab,
    widgets::{LINE, MUTED, Tone, button, error_text, note},
};

impl Client {
    pub(super) fn commit_panel(
        &self,
        tab: &RepoTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = tab.id;
        let oid = tab.history.selected.as_deref().unwrap_or_default();
        let Some(commit) = tab
            .history
            .data
            .position
            .get(oid)
            .and_then(|index| tab.history.data.snapshot.commits.get(*index))
        else {
            return div().child(note(tr!("app.error.commit_outside_history")));
        };
        let detail = match &tab.changes.detail {
            Load::Pending => note(tr!("app.commit.loading")).into_any_element(),
            Load::Failed(error) => error_text(error).into_any_element(),
            Load::Ready(detail) => {
                let scope = Scope::Commit {
                    id: oid.to_owned(),
                    parent: commit.parents.first().cloned(),
                };
                let message_height =
                    (window.viewport_size().height * 0.32).clamp(px(125.), px(375.));
                let message = div().size_full().p(px(8.)).child(
                    div()
                        .size_full()
                        .rounded(px(3.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(0xfafafb))
                        .child(
                            div()
                                .p(px(10.))
                                .text_size(px(14.))
                                .child(detail.message.clone()),
                        )
                        .overflow_y_scrollbar(),
                );
                let files = if tab.changes.show_all_files {
                    match &tab.changes.commit_files {
                        Some(Load::Ready(files)) => self.file_list(tab, files, scope, cx),
                        Some(Load::Failed(error)) => error_text(error),
                        _ => note(tr!("app.commit.loading_tree")),
                    }
                } else {
                    self.file_list(tab, &detail.changes, scope, cx)
                };
                let author_initial = commit
                    .author
                    .chars()
                    .find(|character| !character.is_whitespace())
                    .unwrap_or('?');
                let metadata = div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(9.))
                            .py(px(8.))
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(5.))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .child(
                                        div()
                                            .size(px(31.))
                                            .flex_shrink_0()
                                            .rounded(px(3.))
                                            .bg(rgb(0xe1e8f2))
                                            .text_color(rgb(0x365479))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(author_initial.to_string()),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(commit.author.clone()),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_color(rgb(MUTED))
                                                    .text_size(px(11.))
                                                    .child(tr!(
                                                        "app.commit.authored",
                                                        date = &commit.date
                                                    )),
                                            ),
                                    ),
                            )
                            .when_some(commit.parents.first().cloned(), |element, parent| {
                                element.child(
                                    button(
                                        ("parent", id),
                                        tr!("app.commit.parent", sha = git::short_id(&parent)),
                                        Tone::Plain,
                                        tab.history.data.position.contains_key(&parent),
                                        cx.listener(move |this, _, window, cx| {
                                            this.select(
                                                id,
                                                Some(parent.clone()),
                                                Some(ScrollStrategy::Center),
                                                window,
                                                cx,
                                            );
                                        }),
                                    )
                                    .flex_shrink_0()
                                    .h(px(22.))
                                    .text_size(px(11.)),
                                )
                            }),
                    )
                    .child(change_summary(&detail.changes))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .pr(px(8.))
                            .h(px(34.))
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(self.file_mode(tab, cx).flex_1())
                            .child(
                                div().flex_shrink_0().child(
                                    gpui_kit::component::checkbox::Checkbox::new(("all-files", id))
                                        .small()
                                        .checked(tab.changes.show_all_files)
                                        .label(tr!("app.commit.view_all_files"))
                                        .on_click(cx.listener(
                                            move |this, show: &bool, window, cx| {
                                                this.show_committed_files(id, *show, window, cx);
                                            },
                                        )),
                                ),
                            ),
                    )
                    .child(files);
                div()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_resizable("commit-detail-panes")
                            .with_state(&tab.changes.message_panes)
                            .child(
                                resizable_panel()
                                    .size(message_height)
                                    .size_range(px(84.)..px(650.))
                                    .child(message),
                            )
                            .child(
                                resizable_panel()
                                    .size_range(px(150.)..px(1400.))
                                    .child(metadata),
                            ),
                    )
                    .into_any_element()
            }
        };
        let clipboard_oid = oid.to_owned();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(tab.has_changes(), |element| {
                element.child(
                    div()
                        .h(px(34.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .px(px(5.))
                        .bg(rgb(0xaac4f7))
                        .child(div().truncate().child(tr!(
                            "app.commit.worktree_changes",
                            count = change_count(tab)
                        )))
                        .child(
                            button(
                                ("view-changes", id),
                                tr!("app.commit.view_changes"),
                                Tone::Outline,
                                true,
                                cx.listener(move |this, _, window, cx| {
                                    this.select(id, None, Some(ScrollStrategy::Top), window, cx);
                                }),
                            )
                            .h(px(21.)),
                        ),
                )
            })
            .child(
                div()
                    .h(px(31.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(tr!("app.commit.sha", sha = git::short_id(oid)))
                    .child(
                        button(
                            ("copy-sha", id),
                            tr!("app.commit.copy_sha"),
                            Tone::Plain,
                            true,
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    clipboard_oid.clone(),
                                ));
                            },
                        )
                        .text_size(px(10.)),
                    ),
            )
            .child(detail)
    }
}

fn change_summary(changes: &[Change]) -> Div {
    let mut counts = [0; 4];
    for change in changes {
        counts[match change.code {
            b'A' => 0,
            b'D' => 1,
            b'R' | b'C' => 2,
            _ => 3,
        }] += 1;
    }
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .h(px(32.))
        .flex_shrink_0()
        .px(px(10.))
        .text_size(px(11.))
        .children(
            counts
                .into_iter()
                .zip([
                    (b'A', "app.commit.added"),
                    (b'D', "app.commit.deleted"),
                    (b'R', "app.commit.renamed"),
                    (b'M', "app.commit.modified"),
                ])
                .filter(|(count, _)| *count > 0)
                .map(|(count, (code, key))| {
                    let mark = change_mark(code);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(div().text_color(rgb(mark.color)).child(mark.glyph))
                        .child(tr!(key, count = count))
                }),
        )
}
