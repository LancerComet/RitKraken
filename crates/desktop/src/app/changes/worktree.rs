use gpui_kit::{
    component::{
        Sizable,
        input::{Input, Textarea},
    },
    prelude::*,
    *,
};
use ritkraken_git::{Change, Scope};

use crate::i18n::tr;

use super::{StageAction, StageTarget, change_count};
use crate::app::{
    Client, RepoOperation, RepoTab,
    message::branch_label,
    widgets::{LINE, Tone, button},
};

impl Client {
    pub(super) fn wip_panel(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let snapshot = &tab.history.data.snapshot;
        let status = &snapshot.status;
        let id = tab.id;
        let enabled = tab.commit_enabled(cx);
        let label = match status.staged.len() {
            0 => tr!("app.changes.stage_to_commit"),
            1 => tr!("app.changes.commit_one"),
            count => tr!("app.changes.commit_many", count = count),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .h(px(31.))
                    .px(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(tr!("app.changes.changed_files", count = change_count(tab)))
                    .child(
                        div()
                            .px(px(4.))
                            .min_w_0()
                            .truncate()
                            .bg(rgb(0xbbe4ea))
                            .child(branch_label(snapshot)),
                    ),
            )
            .child(self.file_mode(tab, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(self.file_section(
                        tab,
                        "app.changes.unstaged_files",
                        &status.unstaged,
                        Scope::Worktree,
                        cx,
                    ))
                    .child(self.file_section(
                        tab,
                        "app.changes.staged_files",
                        &status.staged,
                        Scope::Index,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(9.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("app.changes.commit_message")),
                    )
                    .child(Input::new(&tab.changes.summary).small())
                    .child(Textarea::new(&tab.changes.description).small().h(px(63.)))
                    .child(
                        button(
                            ("commit", id),
                            label,
                            Tone::Green,
                            enabled,
                            cx.listener(|this, _, window, cx| this.submit(window, cx)),
                        )
                        .h(px(34.)),
                    ),
            )
    }

    fn file_section(
        &self,
        tab: &RepoTab,
        title_key: &'static str,
        files: &[Change],
        scope: Scope,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = tab.id;
        let action = StageAction::for_scope(&scope).filter(|_| !files.is_empty());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(29.))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_t_1()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!(title_key, count = files.len())),
                    )
                    .when_some(action, |e, action| {
                        e.child(
                            button(
                                title_key,
                                action.label(StageTarget::All),
                                action.tone(),
                                !tab.busy,
                                cx.listener(move |this, _, window, cx| {
                                    let edit = RepoOperation::StageFiles(action, None);
                                    this.update_repo(id, Some(edit), window, cx);
                                }),
                            )
                            .h(px(21.)),
                        )
                    }),
            )
            .child(self.file_list(tab, files, scope, cx))
    }
}
