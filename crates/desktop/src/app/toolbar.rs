use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

use gpui_kit::{prelude::*, *};
use ritkraken_git as git;

use crate::i18n::tr;

use super::{
    Client, RepoOperation, RepoTab,
    widgets::{NAV_TEXT, icon},
};

impl Client {
    pub(super) fn toolbar_actions(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let available = !tab.busy;
        let snapshot = &tab.history.data.snapshot;
        let remote_available = available && !snapshot.remotes.is_empty();
        let remotes = snapshot.remotes.clone();
        let view = cx.weak_entity();
        let fetch_menu = toolbar_button(
            "fetch-options",
            "app.toolbar.fetch_options",
            remote_available,
            cx,
        )
        .w(px(16.))
        .child(icon("icons/chevron-down.svg", NAV_TEXT).size(px(11.)))
        .dropdown_menu(move |menu, _, _| {
            let menu = menu.item(edit_item(
                tr!("app.toolbar.fetch_all"),
                id,
                RepoOperation::Fetch(None),
                &view,
            ));
            remotes.iter().fold(menu, |menu, remote| {
                menu.item(edit_item(
                    tr!("app.toolbar.fetch_remote", remote = remote),
                    id,
                    RepoOperation::Fetch(Some(remote.clone())),
                    &view,
                ))
            })
        });
        let can_stash = available
            && snapshot.head.is_some()
            && tab.has_changes()
            && !snapshot
                .status
                .unstaged
                .iter()
                .any(|change| change.code == b'U');

        div()
            .flex()
            .h_full()
            .items_center()
            .child(
                toolbar_tile(
                    "fetch",
                    "app.toolbar.fetch",
                    "icons/arrow-down-to-line.svg",
                    "app.toolbar.fetch_all",
                    remote_available,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.update_repo(id, Some(RepoOperation::Fetch(None)), window, cx);
                })),
            )
            .child(fetch_menu)
            .child(self.push_button(tab, remote_available, cx))
            .child(
                toolbar_tile(
                    "branch",
                    "app.toolbar.branch",
                    "icons/git-branch.svg",
                    "app.toolbar.create_branch_hint",
                    available && snapshot.head.is_some(),
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_toolbar_branch(id, window, cx);
                })),
            )
            .child(
                toolbar_tile(
                    "stash",
                    "app.toolbar.stash",
                    "icons/download.svg",
                    "app.toolbar.stash_hint",
                    can_stash,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.update_repo(id, Some(RepoOperation::Stash), window, cx);
                })),
            )
            .child(
                toolbar_tile(
                    "pop",
                    "app.toolbar.pop",
                    "icons/upload.svg",
                    "app.toolbar.pop_hint",
                    available && snapshot.has_stash,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.update_repo(id, Some(RepoOperation::Pop), window, cx);
                })),
            )
    }

    fn push_button(
        &self,
        tab: &RepoTab,
        remote_available: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let snapshot = &tab.history.data.snapshot;
        let can_push = remote_available
            && snapshot
                .refs
                .iter()
                .any(|reference| snapshot.is_current_branch(reference));
        let tile = toolbar_tile(
            "push",
            "app.toolbar.push",
            "icons/arrow-up-to-line.svg",
            "app.toolbar.push_hint",
            can_push,
            cx,
        );
        let Some(branch) = snapshot.branch.clone().filter(|_| can_push) else {
            return tile.into_any_element();
        };
        if let Some(upstream) = snapshot
            .upstream
            .as_ref()
            .filter(|upstream| snapshot.remotes.contains(&upstream.remote))
        {
            let edit = RepoOperation::Push {
                branch,
                upstream: upstream.clone(),
                set_upstream: false,
            };
            return tile
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.update_repo(id, Some(edit.clone()), window, cx);
                }))
                .into_any_element();
        }

        let remotes = snapshot.remotes.clone();
        let view = cx.weak_entity();
        tile.dropdown_menu(move |menu, _, _| {
            remotes.iter().fold(menu, |menu, remote| {
                menu.item(edit_item(
                    tr!(
                        "app.toolbar.push_and_track",
                        upstream = format!("{remote}/{branch}")
                    ),
                    id,
                    RepoOperation::Push {
                        branch: branch.clone(),
                        upstream: git::Upstream {
                            remote: remote.clone(),
                            reference: format!("refs/heads/{branch}"),
                        },
                        set_upstream: true,
                    },
                    &view,
                ))
            })
        })
        .into_any_element()
    }
}

fn edit_item(
    label: impl Into<SharedString>,
    id: u64,
    edit: RepoOperation,
    view: &WeakEntity<Client>,
) -> PopupMenuItem {
    let view = view.clone();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| {
            this.update_repo(id, Some(edit.clone()), window, cx)
        });
    })
}

fn toolbar_button(id: &'static str, hint_key: &'static str, enabled: bool, cx: &App) -> Button {
    let hint: SharedString = tr!(hint_key).into();
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(NAV_TEXT).into())
                .hover(rgb(0x3e455e).into())
                .active(rgb(0x3e455e).into()),
        )
        .disabled(!enabled)
        .accessibility_label(hint.clone())
        .tooltip(hint)
        .min_w_0()
        .p(px(0.))
        .rounded(px(0.))
        .h(px(48.))
        .flex_shrink_0()
        .when(!enabled, |button| button.opacity(0.45))
}

fn toolbar_tile(
    id: &'static str,
    label_key: &'static str,
    glyph: &'static str,
    hint_key: &'static str,
    enabled: bool,
    cx: &App,
) -> Button {
    toolbar_button(id, hint_key, enabled, cx).w(px(52.)).child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(5.))
            .child(
                div()
                    .text_size(px(11.))
                    .line_height(px(12.))
                    .text_color(rgb(0xc5cde0))
                    .child(tr!(label_key)),
            )
            .child(icon(glyph, NAV_TEXT).size(px(18.))),
    )
}
