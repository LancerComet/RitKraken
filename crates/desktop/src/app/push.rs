use gpui_kit::{prelude::*, *};

use super::{Client, RepoOperation, RepoTab, theme::palette, toolbar::toolbar_button};
use crate::i18n::tr;

impl Client {
    pub(super) fn push_warning(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Option<Div> {
        let request = tab.push_rejection.as_ref()?;
        if !request.matches_snapshot(&tab.history.data.snapshot) {
            return None;
        }
        let p = palette(cx);
        let id = tab.id;
        let target = format!(
            "{}/{}",
            request.upstream.remote,
            request.upstream.reference.trim_start_matches("refs/heads/")
        );
        let enabled =
            !tab.busy && tab.rebase.is_none() && tab.history.data.snapshot.sequence.is_none();
        Some(
            div()
                .flex_shrink_0()
                .min_h(px(54.))
                .flex()
                .items_center()
                .justify_center()
                .px(px(16.))
                .py(px(8.))
                .bg(rgb(p.chrome_alt))
                .text_color(rgb(p.chrome_text))
                .border_b_1()
                .border_color(rgb(p.border))
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().min_w_0().child(tr!(
                            "app.push.rejected_detail",
                            branch = &request.branch,
                            upstream = target
                        )))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    toolbar_button(
                                        "pull-rejected-push",
                                        "app.push.pull",
                                        enabled,
                                        false,
                                        cx,
                                    )
                                    .h(px(26.))
                                    .px(px(10.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(rgb(p.chrome_border))
                                    .child(tr!("app.push.pull"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.resolve_rejected_push(id, false, window, cx);
                                        },
                                    )),
                                )
                                .child(
                                    toolbar_button(
                                        "force-rejected-push",
                                        "app.push.force",
                                        enabled,
                                        false,
                                        cx,
                                    )
                                    .h(px(26.))
                                    .px(px(10.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(rgb(p.chrome_muted))
                                    .child(tr!("app.push.force"))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.resolve_rejected_push(id, true, window, cx);
                                        },
                                    )),
                                )
                                .child(
                                    toolbar_button(
                                        "cancel-rejected-push",
                                        "app.common.cancel",
                                        true,
                                        false,
                                        cx,
                                    )
                                    .h(px(26.))
                                    .px(px(10.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(rgb(p.chrome_border))
                                    .child(tr!("app.common.cancel"))
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(tab) = this.tab_mut(id) {
                                                tab.push_rejection = None;
                                            }
                                            cx.notify();
                                        },
                                    )),
                                ),
                        ),
                ),
        )
    }

    fn resolve_rejected_push(
        &mut self,
        id: u64,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.active != Some(id) || self.modal_is_present() {
            return;
        }
        let Some(tab) = self.tab_mut(id).filter(|tab| {
            !tab.busy && tab.rebase.is_none() && tab.history.data.snapshot.sequence.is_none()
        }) else {
            return;
        };
        let Some(request) = tab.push_rejection.take() else {
            return;
        };
        let operation = if force {
            RepoOperation::ForcePush(request)
        } else {
            RepoOperation::PullRejected(request)
        };
        self.update_repo(id, Some(operation), window, cx);
    }
}
