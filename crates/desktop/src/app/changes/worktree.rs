use gpui_kit::{
    component::{
        Sizable, Size as InputSize,
        input::{Input, Textarea},
        resizable::{resizable_panel, v_resizable},
    },
    prelude::*,
    *,
};
use ritkraken_git::{Change, Scope};

use crate::i18n::tr;

use super::{StageAction, StageTarget, change_count, files::FILE_ROW_HEIGHT};
use crate::app::{
    Client, RepoOperation, RepoTab,
    message::branch_label,
    theme::palette,
    widgets::{Tone, button},
};

const FILE_SECTION_HEADER_HEIGHT: Pixels = px(39.);
const FILE_SECTION_HEADER_PADDING_Y: Pixels = px(5.);

impl Client {
    pub(super) fn wip_panel(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let snapshot = &tab.history.data.snapshot;
        let status = &snapshot.status;
        let files = div()
            .size_full()
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
            ));
        let message = self.commit_message_form(tab, cx);
        let message_min_height = px(174.);
        let files_min_height = (FILE_SECTION_HEADER_HEIGHT + FILE_ROW_HEIGHT) * 2.;
        let pane_height = tab.changes.worktree_panes.read(cx).container_size();
        let files_min_height = if pane_height > px(0.) {
            files_min_height
                .min((pane_height - message_min_height).max(FILE_SECTION_HEADER_HEIGHT * 2.))
        } else {
            files_min_height
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
                    .gap(px(6.))
                    .h(px(32.))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(tr!("app.changes.changed_files", count = change_count(tab)))
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(1.))
                            .min_w_0()
                            .truncate()
                            .rounded_full()
                            .bg(rgb(p.info_bg))
                            .text_color(rgb(p.info_fg))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(branch_label(snapshot)),
                    ),
            )
            .child(self.file_mode(tab, cx))
            .child(
                div().flex_1().min_h_0().child(
                    v_resizable(("worktree-panes", tab.id))
                        .with_state(&tab.changes.worktree_panes)
                        .child(
                            resizable_panel()
                                .size_range(files_min_height..Pixels::MAX)
                                .child(files),
                        )
                        .child(
                            resizable_panel()
                                .size(px(210.))
                                .size_range(message_min_height..Pixels::MAX)
                                .child(message),
                        ),
                ),
            )
    }

    fn commit_message_form(&self, tab: &RepoTab, cx: &Context<Self>) -> Div {
        let label = match tab.history.data.snapshot.status.staged.len() {
            0 => tr!("app.changes.stage_to_commit"),
            1 => tr!("app.changes.commit_one"),
            count => tr!("app.changes.commit_many", count = count),
        };
        let input_size = InputSize::Small;
        let padding = input_size.input_px() + px(2.);
        // Allow a 20px line, the padding, and a 1px border on each side.
        let summary = Styled::h(
            Input::new(&tab.changes.summary).with_size(input_size),
            px(22.) + padding * 2.,
        )
        .flex_shrink_0()
        .p(padding);
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(9.))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .child(tr!("app.changes.commit_message")),
            )
            .child(summary)
            .child(
                div().flex_1().min_h_0().child(
                    Textarea::new(&tab.changes.description)
                        .with_size(input_size)
                        .h(relative(1.))
                        // The multiline editor already applies the size's padding internally.
                        .px(padding - input_size.input_px())
                        .py(padding - input_size.input_py()),
                ),
            )
            .child(
                button(
                    ("commit", tab.id),
                    label,
                    Tone::Green,
                    tab.commit_enabled(cx),
                    cx,
                    cx.listener(|this, _, window, cx| this.submit(window, cx)),
                )
                .rounded_full()
                .flex_shrink_0()
                .h(px(36.)),
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
        let p = palette(cx);
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
                    .h(FILE_SECTION_HEADER_HEIGHT)
                    .flex_shrink_0()
                    .px(px(10.))
                    .py(FILE_SECTION_HEADER_PADDING_Y)
                    .border_t_1()
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .bg(rgb(p.surface_high))
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
                                cx,
                                cx.listener(move |this, _, window, cx| {
                                    let edit = RepoOperation::StageFiles(action, None);
                                    this.update_repo(id, Some(edit), window, cx);
                                }),
                            )
                            .rounded_full()
                            .h(px(22.)),
                        )
                    }),
            )
            .child(self.file_list(tab, files, scope, cx))
    }
}
