use std::{collections::HashMap, path::PathBuf};

use gpui_kit::*;
use ritkraken_git::{ExportedFile, FileTarget, Scope};

use super::{Client, Message};

#[cfg(windows)]
mod windows;

#[derive(Default)]
pub(super) struct FileOpener {
    snapshots: HashMap<(u64, String, PathBuf), Option<ExportedFile>>,
}

impl Client {
    pub(super) fn open_file(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let Scope::Commit { id: revision, .. } = file.scope else {
            cx.open_with_system(&tab.repo.root().join(&file.change.path));
            return;
        };
        let repo = tab.repo.clone();
        let key = (id, revision.clone(), file.change.path.clone());
        match self.file_opener.snapshots.get(&key) {
            Some(Some(snapshot)) => {
                cx.open_with_system(snapshot.path());
                return;
            }
            Some(None) => return,
            None => {}
        }
        self.file_opener.snapshots.insert(key.clone(), None);
        Self::spawn_background(
            window,
            cx,
            move || repo.export_commit_file(&revision, &file.change.path),
            move |this, result, _, cx| {
                if this.tab(id).is_none() {
                    this.file_opener.snapshots.remove(&key);
                    return;
                }
                match result {
                    Ok(snapshot) => {
                        cx.open_with_system(snapshot.path());
                        this.file_opener.snapshots.insert(key, Some(snapshot));
                    }
                    Err(error) => {
                        this.file_opener.snapshots.remove(&key);
                        this.file_system_error(id, "app.file_menu.open_failed", error, cx);
                    }
                }
            },
        );
    }

    pub(super) fn show_file_in_folder(
        &mut self,
        id: u64,
        path: PathBuf,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !path.exists() {
            if let Some(parent) = path.ancestors().skip(1).find(|parent| parent.is_dir()) {
                #[cfg(windows)]
                match windows::shell_path(parent) {
                    Ok(parent) => cx.open_with_system(&parent),
                    Err(error) => {
                        self.file_system_error(id, "app.file_menu.reveal_failed", error, cx)
                    }
                }
                #[cfg(not(windows))]
                cx.open_with_system(parent);
            }
            return;
        }
        #[cfg(windows)]
        Self::spawn_background(
            window,
            cx,
            move || windows::reveal_file(&path).map_err(|error| error.to_string()),
            move |this, result, _, cx| {
                if let Err(error) = result {
                    this.file_system_error(id, "app.file_menu.reveal_failed", error, cx);
                }
            },
        );
        #[cfg(not(windows))]
        {
            let _ = (id, window);
            cx.reveal_path(&path);
        }
    }

    fn file_system_error(
        &mut self,
        id: u64,
        key: &'static str,
        error: impl ToString,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.tab_mut(id) {
            tab.notice = Some((true, Message::detail(key, error)));
            cx.notify();
        }
    }
}
