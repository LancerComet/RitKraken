use gpui_kit::{
    component::resizable::{h_resizable, resizable_panel},
    prelude::*,
    *,
};

use crate::{
    app::{Client, RepoTab, Submit, widgets::modal::Modal},
    i18n::tr,
};

/// The editor has its own tab identity and view state, outside the workspace tabs.
pub(in crate::app) struct RebaseDialog {
    pub(in crate::app) source_id: u64,
    pub(in crate::app) tab: RepoTab,
    panes: Entity<gpui_kit::base::ResizableState>,
}

impl Client {
    pub(super) fn open_rebase_dialog(
        &mut self,
        source_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let source = self.tab(source_id)?;
        let repo = source.repo.clone();
        let history = source.history.data.clone();
        let limit = source.history.limit;
        let filter = source.history.filter.clone();
        let id = self.session.next_id();
        let tab = RepoTab::new(
            id,
            repo,
            history,
            limit,
            filter,
            &self.saved.preferences,
            window,
            cx,
        );
        self.rebase_modal = Some(Box::new(RebaseDialog {
            source_id,
            tab,
            panes: cx.new(|_| gpui_kit::base::ResizableState::default()),
        }));
        Some(id)
    }

    pub(in crate::app) fn finish_rebase_preparation(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        if let Some(dialog) = &self.rebase_modal
            && dialog.tab.id == id
        {
            let source_id = dialog.source_id;
            self.dismiss_modal(window, cx);
            source_id
        } else {
            id
        }
    }

    pub(in crate::app) fn rebase_dialog_view(
        &self,
        dialog: &RebaseDialog,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Modal {
        let tab = &dialog.tab;
        let id = tab.id;
        let center = if let Some(diff) = &tab.review.active {
            self.diff_view(tab, diff, cx).into_any_element()
        } else {
            self.graph_view(tab, cx).into_any_element()
        };
        Modal::new(
            "interactive-rebase-modal",
            tr!("app.commit_menu.interactive"),
            &self.modal_focus,
            !tab.busy,
            cx.listener(|this, _, window, cx| {
                this.dismiss_modal(window, cx);
                cx.notify();
            }),
        )
        .content_size(size(px(1440.), px(900.)), false)
        .child(
            div()
                .id("interactive-rebase-editor")
                .flex_1()
                .min_h_0()
                .on_action(cx.listener(move |this, _: &Submit, window, cx| {
                    cx.stop_propagation();
                    this.submit_rebase(id, window, cx);
                }))
                .child(
                    h_resizable("rebase-dialog-panes")
                        .with_state(&dialog.panes)
                        .child(
                            resizable_panel()
                                .size_range(px(280.)..px(4000.))
                                .child(center),
                        )
                        .child(
                            resizable_panel()
                                .size(px(350.))
                                .size_range(px(272.)..px(1000.))
                                .child(self.right_panel(tab, window, cx)),
                        ),
                ),
        )
    }
}
