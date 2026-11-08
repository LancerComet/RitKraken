use gpui_kit::*;

use super::RepoOperation;
use crate::{
    app::{Client, Message},
    i18n::tr,
};

impl Client {
    /// Keep watcher refreshes out until the user cancels or the accepted edit starts.
    pub(in crate::app) fn confirm_repo_operation(
        &mut self,
        id: u64,
        operation: RepoOperation,
        title: &str,
        detail: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        let prompt = window.prompt(
            PromptLevel::Warning,
            title,
            Some(detail),
            &[
                PromptButton::cancel(tr!("app.common.cancel")),
                PromptButton::ok(title.to_owned()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |owner, cx| {
            let result = prompt.await;
            let _ = owner.update_in(cx, |client, window, cx| {
                let Some(tab) = client.tab_mut(id) else {
                    return;
                };
                tab.busy = false;
                match result {
                    Ok(1) => client.update_repo(id, Some(operation), window, cx),
                    Ok(_) => {}
                    Err(_) => {
                        tab.notice = Some((true, Message::Key("app.error.confirmation_failed")))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
