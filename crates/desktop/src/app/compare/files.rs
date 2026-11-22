use std::path::Path;

use gpui_kit::{component::scroll::ScrollableElement, prelude::*, *};
use ritkraken_diff::{DiffFile, DiffFileStatus, DiffState, DiffTypography};

use crate::app::{
    changes::{
        FileRow, change_mark, file_row, file_row_icon, file_rows, path_label, path_name,
        tree_mode_pill,
    },
    review::DiffLayout,
    theme::palette,
};

/// The file list + single-file diff viewer shared by the comparison modals:
/// the left column lists a patch's files (flat or grouped into folders) and
/// the right column shows only the selected file's diff.
pub(in crate::app) struct DiffFileBrowser {
    files: Vec<DiffFile>,
    selected: Option<usize>,
    tree: bool,
    scroll: UniformListScrollHandle,
    state: Entity<DiffState>,
    layout: DiffLayout,
    wrap_lines: bool,
    typography: DiffTypography,
}

impl DiffFileBrowser {
    pub(in crate::app) fn new(
        files: Vec<DiffFile>,
        tree: bool,
        layout: DiffLayout,
        wrap_lines: bool,
        typography: DiffTypography,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            files: Vec::new(),
            selected: None,
            tree,
            scroll: UniformListScrollHandle::default(),
            state: cx.new(|cx| DiffState::new(std::iter::empty(), cx)),
            layout,
            wrap_lines,
            typography,
        };
        this.set_files(files, cx);
        this
    }

    /// Replaces the listed files and resets selection to the first file in
    /// display order.
    pub(in crate::app) fn set_files(&mut self, files: Vec<DiffFile>, cx: &mut Context<Self>) {
        self.files = files;
        self.selected = first_file(&paths(&self.files), self.tree);
        self.scroll = UniformListScrollHandle::default();
        self.show_selected(cx);
        cx.notify();
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selected == Some(index) {
            return;
        }
        self.selected = Some(index);
        self.show_selected(cx);
        cx.notify();
    }

    fn set_tree(&mut self, tree: bool, cx: &mut Context<Self>) {
        self.tree = tree;
        cx.notify();
    }

    fn show_selected(&mut self, cx: &mut Context<Self>) {
        let file = self.selected.map(|index| self.files[index].clone());
        let path = file.as_ref().map(|file| file.path().to_string());
        let layout = self.layout;
        self.state.update(cx, |state, cx| {
            // set_files resets scroll and expansion, so the diff opens at the
            // top with the single file expanded.
            state.set_files(file, cx);
            layout.apply(state, cx);
            if let Some(path) = path {
                state.set_file_collapsed(&path, false, cx);
            }
        });
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let rows = file_rows(&paths(&self.files), self.tree);
        let files = self.files.clone();
        let selected = self.selected;
        let tree = self.tree;
        let list_view = cx.weak_entity();
        let pill_view = cx.weak_entity();
        let list = uniform_list("diff-file-browser", rows.len(), move |range, _, cx| {
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
                        let file = &files[*file_index];
                        let mark = change_mark(status_code(file.status()), p);
                        let full_path = display_path(file);
                        let label = if tree {
                            path_name(Path::new(file.path().as_str()))
                        } else {
                            full_path.clone()
                        };
                        let active = selected == Some(*file_index);
                        let file_index = *file_index;
                        file_row(*depth)
                            .id(("file", file_index))
                            .cursor_pointer()
                            .rounded(px(8.))
                            .when(active, |style| style.bg(rgb(p.accent_soft)))
                            .hover(|style| {
                                style.bg(rgb(if active { p.accent_soft_hover } else { p.hover }))
                            })
                            .on_click({
                                let view = list_view.clone();
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.select(file_index, cx);
                                    });
                                }
                            })
                            .child(file_row_icon(mark.icon_path, mark.color))
                            .child(path_label(label, full_path))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .gap(px(4.))
                                    .text_size(px(11.))
                                    .child(
                                        div()
                                            .text_color(rgb(p.success))
                                            .child(format!("+{}", file.additions())),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(p.danger))
                                            .child(format!("−{}", file.deletions())),
                                    ),
                            )
                            .into_any_element()
                    }
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.scroll)
        .size_full();
        div()
            .w(px(280.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(p.border))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px(px(8.))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(tree_mode_pill(0, self.tree, cx, move |tree, _, cx| {
                        let _ = pill_view.update(cx, |this, cx| this.set_tree(tree, cx));
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(px(6.))
                    .py(px(4.))
                    .relative()
                    .child(list)
                    .vertical_scrollbar(&self.scroll),
            )
    }
}

impl Render for DiffFileBrowser {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().child(self.sidebar(cx)).child(
            div().flex_1().min_w_0().h_full().child(
                self.layout
                    .viewer(&self.state, self.wrap_lines, self.typography.clone())
                    .header_visible(true),
            ),
        )
    }
}

/// Maps a parsed file's status onto the status code `change_mark` expects.
fn status_code(status: DiffFileStatus) -> u8 {
    match status {
        DiffFileStatus::Added | DiffFileStatus::Copied => b'A',
        DiffFileStatus::Deleted => b'D',
        DiffFileStatus::Renamed => b'R',
        _ => b'M',
    }
}

fn paths(files: &[DiffFile]) -> Vec<&Path> {
    files
        .iter()
        .map(|file| Path::new(file.path().as_str()))
        .collect()
}

/// The row's full path: `old → new` for renames, the display path otherwise.
fn display_path(file: &DiffFile) -> String {
    match (file.original_path(), file.modified_path()) {
        (Some(old), Some(new)) if old != new => format!("{old} → {new}"),
        _ => file.path().to_string(),
    }
}

/// The file selected initially: the first `File` row in display order, which
/// is sorted by path in tree mode.
fn first_file(paths: &[&Path], tree: bool) -> Option<usize> {
    file_rows(paths, tree).iter().find_map(|row| match row {
        FileRow::File { index, .. } => Some(*index),
        FileRow::Folder { .. } => None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ritkraken_diff::DiffFileStatus;

    use super::{first_file, status_code};
    use crate::app::changes::{FileRow, file_rows};

    #[test]
    fn status_maps_onto_change_codes() {
        assert_eq!(status_code(DiffFileStatus::Added), b'A');
        assert_eq!(status_code(DiffFileStatus::Copied), b'A');
        assert_eq!(status_code(DiffFileStatus::Deleted), b'D');
        assert_eq!(status_code(DiffFileStatus::Renamed), b'R');
        assert_eq!(status_code(DiffFileStatus::Modified), b'M');
        assert_eq!(status_code(DiffFileStatus::Unchanged), b'M');
        assert_eq!(status_code(DiffFileStatus::Conflicted), b'M');
    }

    #[test]
    fn path_mode_rows_keep_input_order_without_folders() {
        let paths = [
            Path::new("b/z.rs"),
            Path::new("a/x.rs"),
            Path::new("b/y.rs"),
        ];
        assert_eq!(
            file_rows(&paths, false),
            vec![
                FileRow::File { index: 0, depth: 0 },
                FileRow::File { index: 1, depth: 0 },
                FileRow::File { index: 2, depth: 0 },
            ]
        );
    }

    #[test]
    fn tree_mode_rows_sort_and_emit_each_folder_once() {
        let paths = [
            Path::new("b/z.rs"),
            Path::new("a/x.rs"),
            Path::new("b/y.rs"),
        ];
        assert_eq!(
            file_rows(&paths, true),
            vec![
                FileRow::Folder {
                    path: Path::new("a").into(),
                    depth: 0
                },
                FileRow::File { index: 1, depth: 1 },
                FileRow::Folder {
                    path: Path::new("b").into(),
                    depth: 0
                },
                FileRow::File { index: 2, depth: 1 },
                FileRow::File { index: 0, depth: 1 },
            ]
        );
    }

    #[test]
    fn first_file_follows_display_order() {
        let paths = [Path::new("b/z.rs"), Path::new("a/x.rs")];
        assert_eq!(first_file(&paths, false), Some(0));
        assert_eq!(first_file(&paths, true), Some(1));
        assert_eq!(first_file(&[], false), None);
    }
}
