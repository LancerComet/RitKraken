use gpui_kit::*;

use super::RepoOperation;
use crate::app::Client;

impl Client {
    pub(in crate::app) fn reset_auto_fetch_timer(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.auto_fetch_task = self
            .saved
            .preferences
            .auto_fetch_interval()
            .map(|interval| {
                let executor = cx.background_executor().clone();
                cx.spawn_in(window, async move |view, cx| {
                    loop {
                        executor.timer(interval).await;
                        if view
                            .update_in(cx, |client, window, cx| {
                                client.auto_fetch_current(window, cx)
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                })
            });
    }

    fn auto_fetch_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.opening || self.modal_is_present() || self.branch_editor.is_editing() {
            return;
        }
        let Some(tab) = self.current() else { return };
        let snapshot = &tab.history.data.snapshot;
        if tab.busy || snapshot.sequence.is_some() || snapshot.remotes.is_empty() {
            return;
        }
        self.start_repo_update(
            tab.id,
            Some(RepoOperation::AutoFetch),
            tab.history.limit,
            false,
            None,
            window,
            cx,
        );
    }
}
