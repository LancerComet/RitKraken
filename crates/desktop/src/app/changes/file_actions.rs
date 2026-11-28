use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{Change, FileTarget, Scope};

use super::{StageAction, inspect::FileInspection, menu::FileAction};
use crate::{
    app::{
        Client, Message, Notice, RepoOperation,
        widgets::delete_dialog::DeleteDialog,
    },
    i18n::tr,
};

impl Client {
    pub(super) fn selected_file_targets(
        &mut self,
        id: u64,
        file: &FileTarget,
        visible: &[Change],
    ) -> Vec<FileTarget> {
        let Some(tab) = self.tab_mut(id) else {
            return Vec::new();
        };
        tab.changes.selection.preview(file);
        tab.changes.selection.selected(&file.scope, visible)
    }

    pub(super) fn stage_selected_files(
        &mut self,
        id: u64,
        files: Vec<FileTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(first) = files.first() else { return };
        if let Some(conflict) = files.iter().find(|file| file.change.code == b'U') {
            self.open_diff(id, conflict.clone(), window, cx);
            return;
        }
        if let Some(action) = StageAction::for_scope(&first.scope) {
            let changes = files.into_iter().map(|file| file.change).collect();
            self.update_repo(
                id,
                Some(RepoOperation::StageFiles(action, Some(changes))),
                window,
                cx,
            );
        }
    }

    pub(super) fn run_file_actions(
        &mut self,
        id: u64,
        files: Vec<FileTarget>,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let Some(file) = files.first().cloned() else {
            return;
        };
        let path = tab.repo.root().join(&file.change.path);
        match action {
            FileAction::CopyPath => {
                let paths = files
                    .iter()
                    .map(|file| {
                        tab.repo
                            .root()
                            .join(&file.change.path)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect::<Vec<_>>();
                cx.write_to_clipboard(ClipboardItem::new_string(paths.join("\n")));
            }
            _ if tab.busy => {}
            FileAction::Stage => self.stage_selected_files(id, files, window, cx),
            FileAction::Discard => self.discard_files(id, files, window, cx),
            FileAction::ExportPatch => self.export_file_patch(id, files, window, cx),
            FileAction::UpdateSubmodules => {
                let paths = files
                    .into_iter()
                    .map(|file| file.change.path)
                    .collect();
                self.update_repo(
                    id,
                    Some(RepoOperation::UpdateSubmodules(Some(paths))),
                    window,
                    cx,
                );
            }
            _ if files.len() != 1 => {}
            FileAction::Open => self.open_file(id, file, window, cx),
            FileAction::Reveal => self.show_file_in_folder(id, path, window, cx),
            FileAction::Ignore(rule, stop_tracking) => self.update_repo(
                id,
                Some(RepoOperation::IgnoreFile(file.change, rule, stop_tracking)),
                window,
                cx,
            ),
            FileAction::Stash => {
                self.update_repo(id, Some(RepoOperation::StashFile(file.change)), window, cx)
            }
            FileAction::Inspect(kind) => {
                FileInspection::open(
                    tab.repo.clone(),
                    file,
                    kind,
                    self.saved.preferences.editor.diff_typography(),
                    window,
                    cx,
                );
            }
        }
    }

    fn prepare_discard_files(
        &mut self,
        id: u64,
        files: Vec<FileTarget>,
        on_ready: impl FnOnce(&mut Self, Vec<ritkraken_git::DiscardFile>, &mut Window, &mut Context<Self>)
        + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if files.is_empty() {
            return;
        }
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || repo.prepare_discards(&files),
            move |this, result, window, cx| {
                let Some(tab) = this.tab_mut(id) else { return };
                tab.busy = false;
                let expected = match result {
                    Ok(value) => value,
                    Err(error) => {
                        tab.notify(Notice::failure(error), window, cx);
                        cx.notify();
                        return;
                    }
                };
                on_ready(this, expected, window, cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(super) fn confirm_discard_unstaged(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_is_present() {
            return;
        }
        let Some(tab) = self.tab(id) else { return };
        let files: Vec<FileTarget> = tab
            .history
            .data
            .snapshot
            .status
            .unstaged
            .iter()
            .cloned()
            .map(|change| FileTarget {
                scope: Scope::Worktree,
                change,
            })
            .collect();
        if files.is_empty() {
            return;
        }
        self.prepare_discard_files(
            id,
            files,
            move |this, expected, window, cx| {
                if expected.is_empty() || this.modal_is_present() {
                    return;
                }
                let message = if expected.len() == 1 {
                    tr!("app.changes.confirm_discard_all_one")
                } else {
                    tr!("app.changes.confirm_discard_all_many", count = expected.len())
                };
                DeleteDialog::open_with_confirm_label(
                    id,
                    tr!("app.changes.confirm_discard_all_title"),
                    message,
                    tr!("app.changes.confirm_discard_all_action"),
                    RepoOperation::DiscardFiles(Arc::new(expected)),
                    window,
                    cx,
                );
            },
            window,
            cx,
        );
    }

    fn discard_files(
        &mut self,
        id: u64,
        files: Vec<FileTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_discard_files(
            id,
            files,
            move |this, expected, window, cx| {
                this.update_repo(
                    id,
                    Some(RepoOperation::DiscardFiles(Arc::new(expected))),
                    window,
                    cx,
                );
            },
            window,
            cx,
        );
    }

    fn export_file_patch(
        &mut self,
        id: u64,
        files: Vec<FileTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let repo = tab.repo.clone();
        let name = if let [file] = files.as_slice() {
            format!(
                "{}.patch",
                file.change
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        } else {
            "changes.patch".to_owned()
        };
        Self::spawn_background(
            window,
            cx,
            move || {
                let mut bytes = Vec::new();
                for file in files {
                    bytes.extend(repo.export_patch(&file)?);
                }
                Ok::<_, ritkraken_git::Error>(bytes)
            },
            move |this, result, window, cx| {
                let bytes = match result {
                    Ok(value) => value,
                    Err(error) => {
                        this.file_action_error(id, error.into(), window, cx);
                        return;
                    }
                };
                let Some(tab) = this.tab_mut(id) else { return };
                if bytes.is_empty() {
                    tab.notify(
                        Notice::info(Message::Key("app.review.no_text_changes")),
                        window,
                        cx,
                    );
                    cx.notify();
                    return;
                }
                this.save_patch(
                    id,
                    name,
                    move |path| {
                        std::fs::write(path, bytes)
                            .map_err(|error| Message::detail("app.file_menu.export_failed", error))
                    },
                    window,
                    cx,
                );
            },
        );
    }

    fn file_action_error(
        &mut self,
        id: u64,
        message: Message,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tab_mut(id) {
            tab.notify(Notice::failure(message), window, cx);
            cx.notify();
        }
    }
}
