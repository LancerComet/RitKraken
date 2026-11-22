use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::{
    component::{scroll::ScrollableElement, tooltip::Tooltip},
    prelude::*,
    *,
};
use ritkraken_git::{Change, FileTarget, Scope};

use crate::i18n::tr;

use super::{StageAction, StageTarget, change_mark, menu::FileMenu};
use crate::app::{
    Client, RepoTab,
    theme::palette,
    widgets::{Tone, button, icon, menu::ContextMenuExt, note},
};

pub(in crate::app) const FILE_ROW_HEIGHT: Pixels = px(32.);

impl Client {
    pub(super) fn file_mode(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let view = cx.weak_entity();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(tree_mode_pill(
                id,
                tab.changes.tree,
                cx,
                move |tree, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if let Some(tab) = this.tab_mut(id) {
                            tab.changes.tree = tree;
                            cx.notify();
                        }
                    });
                },
            ))
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
                .child(note(tr!("app.changes.no_files"), cx));
        }
        let id = tab.id;
        let selected = tab.changes.selection.clone();
        let files = files.to_vec();
        let tree = tab.changes.tree;
        let busy = tab.busy;
        let action = StageAction::for_scope(&scope);
        let paths: Vec<&Path> = files.iter().map(|change| change.path.as_path()).collect();
        let rows = file_rows(&paths, tree);
        let ordered: Arc<Vec<_>> = Arc::new(
            rows.iter()
                .filter_map(|row| match row {
                    FileRow::File { index, .. } => Some(files[*index].clone()),
                    FileRow::Folder { .. } => None,
                })
                .collect(),
        );
        let view = cx.weak_entity();
        let scroll = &tab.changes.scroll[scroll_index];
        let list = uniform_list((label, id), rows.len(), move |range, _, cx| {
            let p = palette(cx);
            range
                .map(|index| match &rows[index] {
                    FileRow::Folder { path, depth } => file_row(*depth)
                        .id(("folder", index))
                        .text_color(rgb(p.text_muted))
                        .child(file_row_icon(Some("icons/folder.svg"), p.text_muted))
                        .child(path_label(
                            path_name(path),
                            path.to_string_lossy().into_owned(),
                        ))
                        .into_any_element(),
                    FileRow::File {
                        index: file_index,
                        depth,
                    } => {
                        let change = &files[*file_index];
                        let full_path = match &change.previous {
                            Some(previous) => {
                                format!("{} → {}", previous.display(), change.path.display())
                            }
                            None => change.path.to_string_lossy().into_owned(),
                        };
                        let path = if tree {
                            path_name(&change.path)
                        } else {
                            full_path.clone()
                        };
                        let file = FileTarget {
                            scope: scope.clone(),
                            change: change.clone(),
                        };
                        let active = selected.contains(&file);
                        let mark = change_mark(change.code, p);
                        let open_view = view.clone();
                        let stage_view = view.clone();
                        let stage_change = change.clone();
                        let stage_file = file.clone();
                        let open_order = ordered.clone();
                        let stage_order = ordered.clone();
                        let menu = FileMenu {
                            id,
                            file: file.clone(),
                            view: view.clone(),
                            files: ordered.clone(),
                        };
                        file_row(*depth)
                            .id(SharedString::from(format!(
                                "{label}-{id}-{}",
                                hex::encode(change.path.as_os_str().as_encoded_bytes())
                            )))
                            .group("file-row")
                            .rounded(px(8.))
                            .cursor_pointer()
                            .when(active, |style| style.bg(rgb(p.accent_soft)))
                            .hover(|style| {
                                style.bg(rgb(if active { p.accent_soft_hover } else { p.hover }))
                            })
                            .on_click(move |event, window, cx| {
                                let modifiers = event.modifiers();
                                let toggle = modifiers.secondary(cx);
                                let _ = open_view.update(cx, |this, cx| {
                                    if let Some(tab) = this.tab_mut(id) {
                                        let selected = tab.changes.selection.click(
                                            &file,
                                            &open_order,
                                            modifiers.shift,
                                            toggle,
                                        );
                                        if selected {
                                            this.open_diff(id, file.clone(), window, cx);
                                        }
                                        cx.notify();
                                    }
                                });
                            })
                            .child(file_row_icon(mark.icon_path, mark.color))
                            .child(path_label(path, full_path))
                            .when_some(action, |row, action| {
                                row.child(
                                    button(
                                        ("file-stage", index),
                                        if stage_change.code == b'U' {
                                            tr!("app.conflicts.resolve")
                                        } else {
                                            action.label(StageTarget::Row)
                                        },
                                        Tone::Plain,
                                        !busy,
                                        cx,
                                        move |_, window, cx| {
                                            cx.stop_propagation();
                                            let _ = stage_view.update(cx, |this, cx| {
                                                let targets = this.selected_file_targets(
                                                    id,
                                                    &stage_file,
                                                    &stage_order,
                                                );
                                                this.stage_selected_files(id, targets, window, cx);
                                            });
                                        },
                                    )
                                    .rounded_full()
                                    .h(px(21.))
                                    .text_size(px(11.))
                                    .text_color(rgb(action.tone().foreground(p)))
                                    .invisible()
                                    .group_hover("file-row", |style| style.visible()),
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
            .px(px(6.))
            .py(px(4.))
            .relative()
            .child(list)
            .vertical_scrollbar(scroll)
    }
}

pub(in crate::app) fn file_row(depth: usize) -> Div {
    div()
        .w_full()
        .h(FILE_ROW_HEIGHT)
        .flex()
        .items_center()
        .pl(px(10. + depth as f32 * 13.))
        .pr(px(10.))
        .gap(px(5.))
        .text_size(px(13.))
}

pub(in crate::app) fn file_row_icon(path: Option<&'static str>, color: u32) -> Div {
    div()
        .size(px(12.))
        .flex_shrink_0()
        .when_some(path, |slot, path| slot.child(icon(path, color).size_full()))
}

pub(in crate::app) fn path_label(label: String, full_path: String) -> Stateful<Div> {
    div()
        .id("file-path")
        .flex_1()
        .min_w_0()
        .truncate()
        .child(label)
        .tooltip(move |window, cx| Tooltip::new(full_path.clone()).build(window, cx))
}

pub(in crate::app) fn path_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum FileRow {
    Folder { path: PathBuf, depth: usize },
    File { index: usize, depth: usize },
}

/// The Path/Tree mode pill shared by this sidebar and the comparison file
/// browser; `on_toggle` receives the newly selected `tree` flag.
pub(in crate::app) fn tree_mode_pill(
    id: u64,
    tree: bool,
    cx: &App,
    on_toggle: impl Fn(bool, &mut Window, &mut App) + Clone + 'static,
) -> Div {
    let p = palette(cx);
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .rounded_full()
        .bg(rgb(p.surface_dim))
        .border_1()
        .border_color(rgb(p.border))
        .children(
            [
                (false, "path-mode", "app.changes.path_mode"),
                (true, "tree-mode", "app.changes.tree_mode"),
            ]
            .into_iter()
            .map(|(mode, name, label)| {
                let on_toggle = on_toggle.clone();
                button(
                    (name, id),
                    tr!(label),
                    Tone::Plain.selected_when(tree == mode),
                    true,
                    cx,
                    move |_, window, cx| on_toggle(mode, window, cx),
                )
                .rounded_full()
                .h(px(22.))
            }),
        )
}

pub(in crate::app) fn file_rows(paths: &[&Path], tree: bool) -> Vec<FileRow> {
    let mut indices: Vec<_> = (0..paths.len()).collect();
    if tree {
        indices.sort_by(|a, b| paths[*a].cmp(paths[*b]));
    }
    let mut folders = HashSet::new();
    let mut rows = Vec::with_capacity(paths.len());
    for index in indices {
        let mut depth = 0;
        if tree {
            let mut folder = PathBuf::new();
            if let Some(parent) = paths[index].parent() {
                for part in parent.components() {
                    folder.push(part.as_os_str());
                    if folders.insert(folder.clone()) {
                        rows.push(FileRow::Folder {
                            path: folder.clone(),
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
