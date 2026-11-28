use gpui_kit::{
    component::{
        Disableable, Sizable, Size as InputSize,
        checkbox::Checkbox,
        input::{Input, Textarea},
        progress::ProgressCircle,
        resizable::resizable_panel,
        tooltip::Tooltip,
    },
    prelude::*,
    *,
};
use ritkraken_git::{Change, FileTarget, Scope};

use crate::i18n::tr;

use super::{StageAction, StageTarget, change_count, files::FILE_ROW_HEIGHT};
use crate::app::{
    Client, RepoTab,
    layout::remembered_size,
    message::branch_label,
    session::ActiveOp,
    theme::palette,
    widgets::{Tone, bento_v_resizable, button, button_content, icon},
};

const FILE_SECTION_HEADER_HEIGHT: Pixels = px(32.);
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
                    .justify_between()
                    .gap(px(8.))
                    .h(px(38.))
                    .flex_shrink_0()
                    .px(px(10.))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(tr!("app.changes.changed_files", count = change_count(tab))),
                            )
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
                    .child(self.file_mode(tab, cx)),
            )
            .child(
                div().flex_1().min_h_0().child(
                    bento_v_resizable(("worktree-panes", tab.id))
                        .with_state(&tab.changes.worktree_panes)
                        .child(
                            resizable_panel()
                                .size_range(files_min_height..Pixels::MAX)
                                .child(files),
                        )
                        .child(
                            resizable_panel()
                                .size(
                                    remembered_size(super::WORKTREE_PANES, 1, cx)
                                        .unwrap_or(px(210.)),
                                )
                                .size_range(message_min_height..Pixels::MAX)
                                .child(message),
                        ),
                ),
            )
    }

    fn commit_message_form(&self, tab: &RepoTab, cx: &Context<Self>) -> Div {
        let p = palette(cx);
        let id = tab.id;
        let amending = tab.changes.amend.is_some();
        let label = if amending {
            tr!("app.changes.amend_commit")
        } else {
            match tab.history.data.snapshot.status.staged.len() {
                0 => tr!("app.changes.stage_to_commit"),
                1 => tr!("app.changes.commit_one"),
                count => tr!("app.changes.commit_many", count = count),
            }
        };
        let input_size = InputSize::Small;
        let padding = input_size.input_px() + px(2.);
        // Allow a 20px line, the padding, and a 1px border on each side.
        let summary = Styled::h(
            Input::new(&tab.changes.summary)
                .with_size(input_size)
                .appearance(false),
            px(22.) + padding * 2.,
        )
        .flex_shrink_0()
        .p(padding);
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .bg(rgb(p.surface_high))
            .border_t_1()
            .border_color(rgb(p.border))
            .child(
                Checkbox::new(("amend-previous-commit", id))
                    .small()
                    .text_size(px(12.))
                    .label(tr!("app.changes.amend_previous"))
                    .checked(amending)
                    .disabled(tab.busy || (!amending && !tab.amend_enabled()))
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        this.toggle_worktree_amend(id, *checked, window, cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(rgb(p.border))
                    .rounded(px(8.))
                    .bg(rgb(p.canvas))
                    .overflow_hidden()
                    .child(summary)
                    .child(
                        div().flex_1().min_h_0().child(
                            Textarea::new(&tab.changes.description)
                                .with_size(input_size)
                                .appearance(false)
                                .h(relative(1.))
                                // Match the summary's inset in the shared message editor.
                                .px(padding - input_size.input_px())
                                .py(padding - input_size.input_py()),
                        ),
                    ),
            )
            .child(
                button_content(
                    ("commit", tab.id),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .when(tab.active_op == Some(ActiveOp::Commit), |row| {
                            row.child(
                                ProgressCircle::new(("commit-spinner", tab.id))
                                    .loading(true)
                                    .small()
                                    .color(rgb(p.accent_fg)),
                            )
                        })
                        .child(div().truncate().child(label)),
                    Tone::Primary,
                    tab.commit_enabled(cx),
                    cx,
                    cx.listener(|this, _, window, cx| this.submit(window, cx)),
                )
                .rounded(px(10.))
                .flex_shrink_0()
                .h(px(34.)),
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
        let selected = tab.changes.selection.selected(&scope, files);
        let (stage_target, targets) = section_stage_targets(&scope, files, selected);
        let conflicted = targets.iter().any(|file| file.change.code == b'U');
        let show_discard_all = shows_discard_all_button(&scope, files);
        let discard_enabled = discard_all_enabled(tab.busy, files);
        let submodules: std::collections::HashSet<std::path::PathBuf> = tab
            .history
            .data
            .snapshot
            .status
            .submodules
            .iter()
            .cloned()
            .collect();
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
                    .gap(px(8.))
                    .h(FILE_SECTION_HEADER_HEIGHT)
                    .flex_shrink_0()
                    .px(px(10.))
                    .py(FILE_SECTION_HEADER_PADDING_Y)
                    .bg(rgb(p.surface_high))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .when(matches!(scope, Scope::Index), |header| {
                        header.border_t_1().border_color(rgb(p.border))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .when(show_discard_all, |row| {
                                row.child(
                                    button_content(
                                        ("discard-all-unstaged", id),
                                        icon("icons/trash.svg", p.text_muted)
                                            .size(px(13.))
                                            .when(discard_enabled, |glyph| {
                                                glyph.group_hover(
                                                    "discard-all-unstaged",
                                                    |style| style.text_color(rgb(p.danger)),
                                                )
                                            }),
                                        Tone::Plain,
                                        discard_enabled,
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            this.confirm_discard_unstaged(id, window, cx);
                                        }),
                                    )
                                    .group("discard-all-unstaged")
                                    .flex_shrink_0()
                                    .size(px(22.))
                                    .px(px(0.))
                                    .rounded(px(6.))
                                    .aria_label(tr!("app.changes.discard_all_unstaged"))
                                    .tooltip(|window, cx| {
                                        Tooltip::new(tr!("app.changes.discard_all_unstaged"))
                                            .build(window, cx)
                                    }),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(tr!(title_key, count = files.len())),
                            ),
                    )
                    .when_some(action, |e, action| {
                        e.child(
                            button(
                                title_key,
                                if conflicted {
                                    tr!("app.conflicts.resolve")
                                } else {
                                    action.label(stage_target)
                                },
                                action.tone(),
                                !tab.busy,
                                cx,
                                cx.listener(move |this, _, window, cx| {
                                    this.stage_selected_files(id, targets.clone(), window, cx);
                                }),
                            )
                            .rounded_full()
                            .flex_shrink_0()
                            .h(px(22.)),
                        )
                    }),
            )
            .child(self.file_list(
                tab,
                files,
                scope,
                &submodules,
                cx,
            ))
    }
}

fn shows_discard_all_button(scope: &Scope, files: &[Change]) -> bool {
    matches!(scope, Scope::Worktree) && !files.is_empty()
}

fn discard_all_enabled(busy: bool, files: &[Change]) -> bool {
    !busy && !files.iter().any(|file| file.code == b'U')
}

fn section_stage_targets(
    scope: &Scope,
    files: &[Change],
    selected: Vec<FileTarget>,
) -> (StageTarget, Vec<FileTarget>) {
    if selected.len() > 1 {
        let count = selected.len();
        (StageTarget::Selected(count), selected)
    } else {
        let targets = files
            .iter()
            .map(|change| FileTarget {
                scope: scope.clone(),
                change: change.clone(),
            })
            .collect();
        (StageTarget::All, targets)
    }
}

#[cfg(test)]
mod tests {
    use ritkraken_git::{Change, FileTarget, Scope};

    use super::{StageTarget, discard_all_enabled, section_stage_targets, shows_discard_all_button};

    fn sample_files() -> Vec<Change> {
        ["a.rs", "b.rs", "c.rs"]
            .map(|path| Change {
                path: path.into(),
                previous: None,
                code: b'M',
            })
            .to_vec()
    }

    #[test]
    fn section_action_stays_all_for_zero_or_one_selected_and_targets_selection_above_one() {
        let files = sample_files();
        for scope in [Scope::Worktree, Scope::Index] {
            let (target, all) = section_stage_targets(&scope, &files, Vec::new());
            assert!(matches!(target, StageTarget::All));
            assert_eq!(all.len(), files.len());

            let one = vec![FileTarget {
                scope: scope.clone(),
                change: files[0].clone(),
            }];
            let (target, still_all) = section_stage_targets(&scope, &files, one);
            assert!(matches!(target, StageTarget::All));
            assert_eq!(still_all.len(), files.len());

            let two = vec![
                FileTarget {
                    scope: scope.clone(),
                    change: files[0].clone(),
                },
                FileTarget {
                    scope: scope.clone(),
                    change: files[2].clone(),
                },
            ];
            let (target, subset) = section_stage_targets(&scope, &files, two);
            assert!(matches!(target, StageTarget::Selected(2)));
            assert_eq!(
                subset
                    .iter()
                    .map(|item| item.change.path.to_str().unwrap())
                    .collect::<Vec<_>>(),
                ["a.rs", "c.rs"]
            );
        }
    }

    #[test]
    fn discard_all_button_shows_only_for_populated_worktree_sections() {
        let files = sample_files();
        assert!(shows_discard_all_button(&Scope::Worktree, &files));
        assert!(!shows_discard_all_button(&Scope::Index, &files));
        let commit = Scope::Commit {
            id: "id".into(),
            parent: None,
        };
        assert!(!shows_discard_all_button(&commit, &files));
        assert!(!shows_discard_all_button(&Scope::Worktree, &[]));
    }

    #[test]
    fn discard_all_disabled_when_busy_or_conflicted() {
        let files = sample_files();
        assert!(discard_all_enabled(false, &files));
        assert!(!discard_all_enabled(true, &files));
        let mut conflicted = files.clone();
        conflicted[1].code = b'U';
        assert!(!discard_all_enabled(false, &conflicted));
    }
}
