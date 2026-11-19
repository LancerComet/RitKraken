use gpui_kit::{
    component::resizable::resizable_panel,
    prelude::*,
    *,
};

use super::{
    Client, RepoTab,
    layout::{remembered_panes, remembered_size},
    widgets::bento_h_resizable,
};

pub(super) struct WorkspacePanes {
    workspace: Entity<gpui_kit::base::ResizableState>,
    navigation: Entity<gpui_kit::base::ResizableState>,
}

const WORKSPACE_PANES: &str = "workspace-panes";
const NAVIGATION_PANES: &str = "navigation-panes";

impl WorkspacePanes {
    pub(super) fn new(cx: &mut Context<Client>) -> Self {
        Self {
            workspace: remembered_panes(WORKSPACE_PANES, cx),
            navigation: remembered_panes(NAVIGATION_PANES, cx),
        }
    }
}

impl Client {
    pub(super) fn workspace(
        &self,
        tab: &RepoTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width = window.viewport_size().width;
        let right = remembered_size(WORKSPACE_PANES, 1, cx)
            .unwrap_or_else(|| (width * 0.30).clamp(px(296.), px(570.)));
        let left = remembered_size(NAVIGATION_PANES, 0, cx)
            .unwrap_or_else(|| (width * 0.175).clamp(px(168.), px(320.)));
        let center = div()
            .size_full()
            .pl(px(4.))
            .pr(px(4.))
            .child(if let Some(diff) = &tab.review.active {
                self.diff_view(tab, diff, cx).into_any_element()
            } else {
                self.graph_view(tab, cx).into_any_element()
            });
        let collapsed = tab
            .review
            .active
            .as_ref()
            .map_or(tab.refs.collapsed, |diff| diff.collapsed_sidebar);
        let navigation = if collapsed {
            div()
                .size_full()
                .flex()
                .child(
                    div()
                        .h_full()
                        .flex_shrink_0()
                        .pr(px(4.))
                        .child(self.branch_rail(tab, cx)),
                )
                .child(div().flex_1().min_w_0().h_full().child(center))
                .into_any_element()
        } else {
            bento_h_resizable(NAVIGATION_PANES)
                .with_state(&self.panes.navigation)
                .child(
                    resizable_panel()
                        .size(left)
                        .size_range(px(140.)..px(490.))
                        .child(div().size_full().pr(px(4.)).child(self.sidebar(tab, cx))),
                )
                .child(
                    resizable_panel()
                        .size_range(px(280.)..px(4000.))
                        .child(center),
                )
                .into_any_element()
        };
        bento_h_resizable(WORKSPACE_PANES)
            .with_state(&self.panes.workspace)
            .child(
                resizable_panel()
                    .size_range(px(420.)..px(4000.))
                    .child(navigation),
            )
            .child(
                resizable_panel()
                    .size(right)
                    .size_range(px(272.)..px(1000.))
                    .child(
                        div()
                            .size_full()
                            .pl(px(4.))
                            .child(self.right_panel(tab, window, cx)),
                    ),
            )
    }
}
