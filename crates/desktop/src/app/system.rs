use std::path::PathBuf;

use gpui_kit::*;

use super::{Client, Message};

#[cfg(windows)]
mod windows;

impl Client {
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
                    Err(error) => self.file_reveal_error(id, error, cx),
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
                    this.file_reveal_error(id, error, cx);
                }
            },
        );
        #[cfg(not(windows))]
        {
            let _ = (id, window);
            cx.reveal_path(&path);
        }
    }

    fn file_reveal_error(&mut self, id: u64, error: impl ToString, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab_mut(id) {
            tab.notice = Some((true, Message::detail("app.file_menu.reveal_failed", error)));
            cx.notify();
        }
    }
}
