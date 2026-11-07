use std::{collections::HashSet, path::PathBuf};

use gpui_kit::{component::scroll::ScrollableElement, prelude::*, *};
use ritkraken_git::{Change, FileTarget, Scope};

use crate::i18n::tr;

use super::{StageAction, StageTarget, change_mark, menu::FileMenu};
use crate::app::{
    Client, RepoOperation, RepoTab,
    widgets::{HOVER, MUTED, PAPER, SELECT, Tone, button, icon, menu::ContextMenuExt, note},
};

pub(super) const FILE_ROW_HEIGHT: Pixels = px(32.);

impl Client {
    pub(super) fn file_mode(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        div()
            .h(px(34.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .children(
                [
                    (false, "path-mode", "app.changes.path_mode"),
                    (true, "tree-mode", "app.changes.tree_mode"),
                ]
                .into_iter()
                .map(|(tree, name, label)| {
                    button(
                        (name, id),
                        tr!(label),
                        Tone::Outline.selected_when(tab.changes.tree == tree),
                        true,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(tab) = this.tab_mut(id) {
                                tab.changes.tree = tree;
                                cx.notify();
                            }
                        }),
                    )
                }),
            )
    }

    pub(super) fn file_list(
        &self,
        tab: &RepoTab,
        files: &[Change],
        scope: Scope,
        cx: &mut Context<Self>,
    ) -> Div {
        let (label, scroll_index) = match &scope {
            Scope::Worktree => ("worktree-files", 0),
            Scope::Index => ("staged-files", 1),
            Scope::Commit { .. } => ("commit-files", 2),
        };
        if files.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .child(note(tr!("app.changes.no_files")));
        }
        let id = tab.id;
        let selected = tab.review.active.as_ref().map(|diff| diff.file.clone());
        let files = files.to_vec();
        let tree = tab.changes.tree;
        let busy = tab.busy;
        let action = StageAction::for_scope(&scope);
        let rows = file_rows(&files, tree);
        let view = cx.weak_entity();
        let scroll = &tab.changes.scroll[scroll_index];
        let list = uniform_list((label, id), rows.len(), move |range, _, _| {
            range
                .map(|index| match &rows[index] {
                    FileRow::Folder { name, depth } => file_row(*depth)
                        .text_color(rgb(MUTED))
                        .child(file_row_icon(Some("icons/folder.svg"), MUTED))
                        .child(div().truncate().child(name.clone()))
                        .into_any_element(),
                    FileRow::File {
                        index: file_index,
                        depth,
                    } => {
                        let change = &files[*file_index];
                        let path = if tree {
                            change
                                .path
                                .file_name()
                                .unwrap_or(change.path.as_os_str())
                                .to_string_lossy()
                                .into_owned()
                        } else if let Some(previous) = &change.previous {
                            format!("{} → {}", previous.display(), change.path.display())
                        } else {
                            change.path.to_string_lossy().into_owned()
                        };
                        let file = FileTarget {
                            scope: scope.clone(),
                            change: change.clone(),
                        };
                        let active = selected.as_ref() == Some(&file);
                        let mark = change_mark(change.code);
                        let open_view = view.clone();
                        let stage_view = view.clone();
                        let stage_change = change.clone();
                        let menu = FileMenu {
                            id,
                            file: file.clone(),
                            view: view.clone(),
                        };
                        file_row(*depth)
                            .id(SharedString::from(format!(
                                "{label}-{id}-{}",
                                hex::encode(change.path.as_os_str().as_encoded_bytes())
                            )))
                            .cursor_pointer()
                            .bg(rgb(if active { SELECT } else { PAPER }))
                            .hover(|style| style.bg(rgb(HOVER)))
                            .on_click(move |_, window, cx| {
                                let _ = open_view.update(cx, |this, cx| {
                                    this.open_diff(id, file.clone(), window, cx)
                                });
                            })
                            .child(file_row_icon(mark.icon_path, mark.color))
                            .child(div().flex_1().min_w_0().truncate().child(path))
                            .when_some(action, |row, action| {
                                row.child(
                                    button(
                                        ("file-stage", index),
                                        action.label(StageTarget::Row),
                                        Tone::Plain,
                                        !busy,
                                        move |_, window, cx| {
                                            cx.stop_propagation();
                                            let _ = stage_view.update(cx, |this, cx| {
                                                let edit = RepoOperation::StageFiles(
                                                    action,
                                                    Some(stage_change.clone()),
                                                );
                                                this.update_repo(id, Some(edit), window, cx);
                                            });
                                        },
                                    )
                                    .h(px(21.))
                                    .text_size(px(11.))
                                    .text_color(rgb(action.tone().foreground())),
                                )
                            })
                            .context_menu(move |popup, window, cx| menu.build(popup, window, cx))
                            .into_any_element()
                    }
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .mx(px(10.))
            .relative()
            .child(list)
            .vertical_scrollbar(scroll)
    }
}

fn file_row(depth: usize) -> Div {
    div()
        .w_full()
        .h(FILE_ROW_HEIGHT)
        .flex()
        .items_center()
        .pl(px(10. + depth as f32 * 13.))
        .pr(px(10.))
        .gap(px(5.))
        .text_size(px(14.))
}

fn file_row_icon(path: Option<&'static str>, color: u32) -> Div {
    div()
        .size(px(12.))
        .flex_shrink_0()
        .when_some(path, |slot, path| slot.child(icon(path, color).size_full()))
}

enum FileRow {
    Folder { name: String, depth: usize },
    File { index: usize, depth: usize },
}

fn file_rows(files: &[Change], tree: bool) -> Vec<FileRow> {
    let mut indices: Vec<_> = (0..files.len()).collect();
    if tree {
        indices.sort_by(|a, b| files[*a].path.cmp(&files[*b].path));
    }
    let mut folders = HashSet::new();
    let mut rows = Vec::with_capacity(files.len());
    for index in indices {
        let mut depth = 0;
        if tree {
            let mut folder = PathBuf::new();
            if let Some(parent) = files[index].path.parent() {
                for part in parent.components() {
                    folder.push(part.as_os_str());
                    if folders.insert(folder.clone()) {
                        rows.push(FileRow::Folder {
                            name: part.as_os_str().to_string_lossy().into_owned(),
                            depth,
                        });
                    }
                    depth += 1;
                }
            }
        }
        rows.push(FileRow::File { index, depth });
    }
    rows
}
