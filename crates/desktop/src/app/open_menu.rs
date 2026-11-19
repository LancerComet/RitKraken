use std::path::PathBuf;

use gpui_kit::component::{popover::Popover, scroll::ScrollableElement};
use gpui_kit::{prelude::*, *};
use ritkraken_git::Repo;

use crate::i18n::tr;

use super::{
    Client, Tab, repository_manager::remove_recent_button, theme::Palette, toolbar::toolbar_tile,
    widgets::icon,
};

impl Client {
    pub(super) fn open_repo_menu(
        &self,
        tab: Option<&Tab>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger = toolbar_tile(
            "open-repo-menu",
            "app.common.open",
            "icons/folder-open.svg",
            "app.repositories.open_manager",
            true,
            false,
            cx,
        );
        let recent = self.saved.recent.clone();
        let active_root: Option<PathBuf> = tab.map(|tab| tab.root().to_path_buf());
        let opening = self.session.opening;
        let client = cx.weak_entity();

        Popover::new("open-repo-popover")
            .anchor(Anchor::TopLeft)
            .offset(px(6.))
            .when(self.modal_is_present(), |popover| popover.open(false))
            .p(px(0.))
            .w(px(312.))
            .max_h(px(320.))
            .rounded(px(14.))
            .overflow_hidden()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.border))
            .trigger(trigger)
            .content(move |_, _, popover_cx| {
                let open_client = client.clone();
                let open_shortcut = if cfg!(target_os = "macos") {
                    "⌘O"
                } else {
                    "Ctrl+O"
                };
                let accent_fg = if p.is_dark {
                    p.accent
                } else {
                    p.accent_soft_fg
                };

                div()
                    .w_full()
                    .max_h(px(318.))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .text_color(rgb(p.text))
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex_shrink_0()
                            .p(px(6.))
                            .border_b_1()
                            .border_color(rgb(p.border))
                            .child(
                                div()
                                    .id("open-menu-open-action")
                                    .w_full()
                                    .h(px(36.))
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .when(opening, |row| row.opacity(0.5))
                                    .when(!opening, |row| {
                                        row.cursor_pointer()
                                            .hover(|s| s.bg(rgb(p.hover)))
                                            .on_click(popover_cx.listener(
                                                move |state, _, window, cx| {
                                                    state.dismiss(window, cx);
                                                    let _ = open_client.update(cx, |this, cx| {
                                                        this.open_manager(window, cx);
                                                    });
                                                },
                                            ))
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .size(px(24.))
                                                    .flex_shrink_0()
                                                    .rounded(px(6.))
                                                    .bg(rgb(p.accent_soft))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        icon("icons/folder-open.svg", accent_fg)
                                                            .size(px(14.)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(tr!("app.common.open")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .px(px(6.))
                                            .py(px(2.))
                                            .rounded(px(5.))
                                            .bg(rgb(p.surface_dim))
                                            .text_size(px(10.))
                                            .text_color(rgb(p.text_subtle))
                                            .child(open_shortcut),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(12.))
                            .pt(px(8.))
                            .pb(px(4.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(p.text_subtle))
                            .child(tr!("app.repositories.recent_heading"))
                            .child(recent.len().to_string()),
                    )
                    .child(if recent.is_empty() {
                        div()
                            .px(px(12.))
                            .py(px(18.))
                            .text_center()
                            .text_color(rgb(p.text_subtle))
                            .child(tr!("app.repositories.empty_list"))
                            .into_any_element()
                    } else {
                        div()
                            .w_full()
                            .h_auto()
                            .flex_grow_1()
                            .flex_shrink_1()
                            .min_h_0()
                            .max_h(px(242.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .px(px(6.))
                            .pb(px(6.))
                            .children(recent.iter().enumerate().map(|(index, path)| {
                                let is_current = active_root.as_deref() == Some(path.as_path());
                                let row_path = path.clone();
                                let remove_path = path.clone();
                                let row_client = client.clone();
                                let remove_client = client.clone();
                                let remove_group: SharedString =
                                    format!("open-menu-remove-{index}").into();
                                let icon_bg = if is_current { p.accent } else { p.surface_dim };
                                let icon_border = if is_current { p.accent } else { p.border };
                                let icon_fg = if is_current {
                                    p.accent_fg
                                } else {
                                    p.text_muted
                                };
                                let name_fg = if is_current { accent_fg } else { p.text };

                                div()
                                    .id(("open-menu-recent", index))
                                    .h(px(42.))
                                    .w_full()
                                    .flex_shrink_0()
                                    .px(px(8.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .when(is_current, |row| row.bg(rgb(p.accent_soft)))
                                    .when(!opening, |row| {
                                        row.cursor_pointer()
                                            .when(!is_current, |row| {
                                                row.hover(|s| s.bg(rgb(p.hover)))
                                            })
                                            .on_click(popover_cx.listener(
                                                move |state, _, window, cx| {
                                                    state.dismiss(window, cx);
                                                    let _ = row_client.update(cx, |this, cx| {
                                                        this.open_recent_repo(
                                                            row_path.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                },
                                            ))
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .size(px(24.))
                                                    .flex_shrink_0()
                                                    .rounded(px(6.))
                                                    .bg(rgb(icon_bg))
                                                    .border_1()
                                                    .border_color(rgb(icon_border))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        icon("icons/git-branch.svg", icon_fg)
                                                            .size(px(12.)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_size(px(12.))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(name_fg))
                                                            .child(Repo::name_from_path(path)),
                                                    )
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_size(px(10.))
                                                            .text_color(rgb(p.text_subtle))
                                                            .child(
                                                                path.to_string_lossy()
                                                                    .into_owned(),
                                                            ),
                                                    ),
                                            ),
                                    )
                                    .child(remove_recent_button(
                                        ("open-menu-remove-recent", index),
                                        remove_group,
                                        p,
                                        move |_, window, cx| {
                                            let _ = remove_client.update(cx, |this, cx| {
                                                this.remove_recent_repo(&remove_path, window, cx);
                                            });
                                        },
                                    ))
                            }))
                            .overflow_y_scrollbar()
                            .id("open-menu-recent-scroll")
                            .into_any_element()
                    })
            })
            .into_any_element()
    }
}
