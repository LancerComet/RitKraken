use std::path::PathBuf;

use super::{Client, Message, Notice};
use gpui_kit::*;

impl Client {
    /// One save picker, asynchronous writer and result reporting path for every
    /// patch export, including worktree files and historical commits.
    pub(super) fn save_patch(
        &mut self,
        id: u64,
        name: String,
        write: impl FnOnce(PathBuf) -> Result<(), Message> + Send + 'static,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let prompt = cx.prompt_for_new_path(tab.repo.root(), Some(&name));
        cx.spawn_in(window, async move |view, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(path))) => {
                    cx.background_executor()
                        .spawn(async move { write(path) })
                        .await
                }
                Ok(Err(error)) => Err(Message::detail("app.file_menu.export_failed", error)),
                _ => return,
            };
            let _ = view.update_in(cx, |this, window, cx| {
                if let Some(tab) = this.tab_mut(id) {
                    tab.notify(
                        match result {
                            Ok(()) => Notice::info(Message::Key("app.file_menu.patch_saved")),
                            Err(error) => Notice::failure(error),
                        },
                        window,
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
