use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

use gpui_kit::{component::input::Input, prelude::*, *};
use ritkraken_git as git;

use super::{
    Client, RepoOperation, RepoTab,
    widgets::{NAV_TEXT, Tone, WHITE, button, icon, modal_backdrop},
};

impl Client {
    pub(super) fn toolbar_actions(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let available = !tab.busy;
        let snapshot = &tab.history.data.snapshot;
        let remote_available = available && !snapshot.remotes.is_empty();
        let remotes = snapshot.remotes.clone();
        let view = cx.weak_entity();
        let fetch_menu = toolbar_button("fetch-options", "Fetch options", remote_available, cx)
            .w(px(16.))
            .child(icon("icons/chevron-down.svg", NAV_TEXT).size(px(11.)))
            .dropdown_menu(move |menu, _, _| {
                let menu = menu.item(edit_item(
                    "Fetch All",
                    id,
                    RepoOperation::Fetch(None),
                    &view,
                ));
                remotes.iter().fold(menu, |menu, remote| {
                    menu.item(edit_item(
                        format!("Fetch {remote}"),
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
                    "Fetch",
                    "icons/arrow-down-to-line.svg",
                    "Fetch All",
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
                    "Branch",
                    "icons/git-branch.svg",
                    "Create branch",
                    available,
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.branch_dialog.open = true;
                    this.branch_dialog.name.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus_handle(cx).focus(window, cx);
                    });
                    cx.notify();
                })),
            )
            .child(
                toolbar_tile(
                    "stash",
                    "Stash",
                    "icons/download.svg",
                    "Stash changes",
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
                    "Pop",
                    "icons/upload.svg",
                    "Pop latest stash",
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
            "Push",
            "icons/arrow-up-to-line.svg",
            "Push current branch",
            can_push,
            cx,
        );
        if !can_push {
            return tile.into_any_element();
        }
        let branch = snapshot.branch.clone();
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
                    format!("Push and track {remote}/{branch}"),
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

fn toolbar_button(id: &'static str, hint: &'static str, enabled: bool, cx: &App) -> Button {
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(NAV_TEXT).into())
                .hover(rgb(0x3e455e).into())
                .active(rgb(0x3e455e).into()),
        )
        .disabled(!enabled)
        .accessibility_label(hint)
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
    label: &'static str,
    glyph: &'static str,
    hint: &'static str,
    enabled: bool,
    cx: &App,
) -> Button {
    toolbar_button(id, hint, enabled, cx).w(px(52.)).child(
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
                    .child(label),
            )
            .child(icon(glyph, NAV_TEXT).size(px(18.))),
    )
}

pub(super) struct BranchDialog {
    pub(super) open: bool,
    name: Entity<gpui_kit::component::input::InputState>,
}

impl BranchDialog {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let name = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx).placeholder("Branch name")
        });
        Self { open: false, name }
    }
}

impl Client {
    pub(super) fn branch_view(&self, cx: &mut Context<Self>) -> Div {
        modal_backdrop().child(
            div()
                .w(px(390.))
                .p(px(15.))
                .rounded(px(4.))
                .shadow_lg()
                .bg(rgb(WHITE))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(16.))
                        .child("Create and check out a branch"),
                )
                .child(Input::new(&self.branch_dialog.name))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6.))
                        .child(button(
                            "cancel-branch",
                            "Cancel",
                            Tone::Outline,
                            true,
                            cx.listener(|this, _, window, cx| this.escape(window, cx)),
                        ))
                        .child(button(
                            "create-branch",
                            "Create Branch",
                            Tone::Green,
                            true,
                            cx.listener(|this, _, window, cx| {
                                let branch =
                                    this.branch_dialog.name.read(cx).value().trim().to_owned();
                                if let Some(id) = this.session.active.filter(|_| !branch.is_empty())
                                {
                                    this.branch_dialog.open = false;
                                    this.focus.focus(window, cx);
                                    this.update_repo(
                                        id,
                                        Some(RepoOperation::Branch(branch)),
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )),
                ),
        )
    }
}
