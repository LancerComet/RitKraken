use gpui_kit::{
    component::{
        Sizable,
        resizable::{resizable_panel, v_resizable},
        scroll::ScrollableElement,
        tooltip::Tooltip,
    },
    prelude::*,
    *,
};
use ritkraken_git::{self as git, Scope};

use crate::i18n::tr;

use super::{ChangeSummary, SummaryStyle, change_count};
use crate::app::{
    Client, Load, RepoTab,
    theme::palette,
    widgets::{Tone, button, error_text, note},
};

impl Client {
    pub(super) fn commit_panel(
        &self,
        tab: &RepoTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let id = tab.id;
        let oid = tab.history.selected.as_deref().unwrap_or_default();
        let Some(commit) = tab
            .history
            .data
            .position
            .get(oid)
            .and_then(|index| tab.history.data.snapshot.commits.get(*index))
        else {
            return div().child(note(tr!("app.error.commit_outside_history"), cx));
        };
        let detail = match &tab.changes.detail {
            Load::Pending => note(tr!("app.commit.loading"), cx).into_any_element(),
            Load::Failed(error) => error_text(error, cx).into_any_element(),
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
                        .rounded(px(12.))
                        .border_1()
                        .border_color(rgb(p.border))
                        .bg(rgb(p.surface))
                        .child(
                            div()
                                .p(px(12.))
                                .text_size(px(14.))
                                .text_color(rgb(p.text))
                                .child(detail.message.clone()),
                        )
                        .overflow_y_scrollbar(),
                );
                let files = if tab.changes.show_all_files {
                    match &tab.changes.commit_files {
                        Some(Load::Ready(files)) => self.file_list(tab, files, scope, cx),
                        Some(Load::Failed(error)) => error_text(error, cx),
                        _ => note(tr!("app.commit.loading_tree"), cx),
                    }
                } else {
                    self.file_list(tab, &detail.changes, scope, cx)
                };
                let metadata = div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.commit_people(tab, commit, detail, cx))
                    .child(ChangeSummary::new(&detail.changes).view(SummaryStyle::Labeled, p))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .pr(px(8.))
                            .h(px(36.))
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(rgb(p.border))
                            .child(self.file_mode(tab, cx).flex_1())
                            .child(
                                div().flex_shrink_0().child(
                                    gpui_kit::component::checkbox::Checkbox::new(("all-files", id))
                                        .small()
                                        .text_size(px(13.))
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
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(tab.has_changes(), |element| {
                element.child(
                    div()
                        .h(px(36.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .bg(rgb(p.info_bg))
                        .text_color(rgb(p.info_fg))
                        .border_b_1()
                        .border_color(rgb(p.border))
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
                                cx,
                                cx.listener(move |this, _, window, cx| {
                                    this.select(id, None, Some(ScrollStrategy::Top), window, cx);
                                }),
                            )
                            .rounded_full()
                            .h(px(22.)),
                        ),
                )
            })
            .child(
                div()
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(commit_hash_button(id, oid, cx)),
            )
            .child(detail)
    }
}

fn commit_hash_button(id: u64, oid: &str, cx: &App) -> Stateful<Div> {
    let clipboard_oid = oid.to_owned();
    let horizontal_padding = px(10.);
    let press_offset = px(1.);
    button(
        ("copy-sha", id),
        git::short_id(oid).to_owned(),
        Tone::Plain,
        true,
        cx,
        move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(clipboard_oid.clone()));
        },
    )
    .rounded_full()
    .role(Role::Button)
    .aria_label(tr!("app.commit.copy_sha"))
    .px(horizontal_padding)
    .py(press_offset)
    .active(|style| {
        style
            .pl(horizontal_padding + press_offset)
            .pr(horizontal_padding - press_offset)
            .pt(press_offset * 2.)
            .pb_0()
    })
    .tooltip(|window, cx| Tooltip::new(tr!("app.commit.copy_sha")).build(window, cx))
}
