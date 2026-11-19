use gpui_kit::component::{
    Disableable, Selectable as _,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
    progress::ProgressCircle,
};

use gpui_kit::{prelude::*, *};
use ritkraken_git as git;

use crate::i18n::tr;

use super::{
    Client, OpenPalette, OpenPreferences, RepoOperation, RepoTab, SearchCommits,
    palette::{Command, CommandContext},
    session::ActiveOp,
    theme::palette,
    widgets::icon,
};

impl Client {
    pub(super) fn toolbar_actions(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let context = Some(CommandContext::from(tab));
        let active_op = tab.active_op;
        let remote_available = Command::Fetch.available(context);
        let can_stash = Command::Stash.available(context);

        div()
            .flex()
            .h_full()
            .items_center()
            .gap(px(4.))
            .child(
                toolbar_tile(
                    "worktrees",
                    "app.worktrees.short_title",
                    "icons/folder-symlink.svg",
                    "app.worktrees.title",
                    Command::Worktrees.available(context),
                    false,
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_worktrees(window, cx))),
            )
            .child(
                toolbar_tile(
                    "recovery",
                    "app.recovery.short_title",
                    "icons/undo.svg",
                    "app.recovery.title",
                    Command::Recovery.available(context),
                    false,
                    cx,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_recovery(window, cx))),
            )
            .child(toolbar_divider(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(0.))
                    .children(self.pull_controls(tab, remote_available, cx)),
            )
            .child(self.push_button(tab, remote_available, cx))
            .child(toolbar_divider(cx))
            .child(
                toolbar_tile(
                    "branch",
                    "app.toolbar.branch",
                    "icons/git-branch.svg",
                    "app.toolbar.create_branch_hint",
                    Command::CreateBranch.available(context),
                    active_op == Some(ActiveOp::Branch),
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
                    active_op == Some(ActiveOp::Stash),
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
                    Command::Pop.available(context),
                    active_op == Some(ActiveOp::Pop),
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.update_repo(id, Some(RepoOperation::Pop), window, cx);
                })),
            )
    }

    pub(super) fn toolbar_navigation(&self, tab: Option<&RepoTab>, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.))
            .child(
                toolbar_tile(
                    "search",
                    "app.search.short_title",
                    "icons/search.svg",
                    "app.search.title",
                    Command::SearchCommits.available(tab.map(CommandContext::from)),
                    false,
                    cx,
                )
                .tooltip_with_action(tr!("app.search.title"), &SearchCommits, Some("CommitGraph"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_commit_search(window, cx);
                })),
            )
            .child(
                toolbar_tile(
                    "commands",
                    "app.palette.short_title",
                    "icons/command.svg",
                    "app.palette.title",
                    true,
                    false,
                    cx,
                )
                .tooltip_with_action(tr!("app.palette.title"), &OpenPalette, Some("RitKraken"))
                .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(
                toolbar_tile(
                    "preferences",
                    "app.preferences.short_title",
                    "icons/settings.svg",
                    "app.preferences.title",
                    !self.session.opening,
                    false,
                    cx,
                )
                .selected(self.preferences.is_some())
                .tooltip_with_action(tr!("app.preferences.title"), &OpenPreferences, Some("RitKraken"))
                .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx))),
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
        let spinning = tab.active_op == Some(ActiveOp::Push);
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
            spinning,
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

pub(super) fn toolbar_button(
    id: &'static str,
    hint_key: &'static str,
    enabled: bool,
    spinning: bool,
    cx: &App,
) -> Button {
    let p = palette(cx);
    let hint: SharedString = tr!(hint_key).into();
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(if spinning {
                    rgb(p.chrome_hover).into()
                } else {
                    transparent_black()
                })
                .foreground(rgb(p.chrome_text).into())
                .hover(rgb(p.chrome_hover).into())
                .active(rgb(p.chrome_hover).into()),
        )
        .disabled(!enabled || spinning)
        .accessibility_label(hint.clone())
        .tooltip(hint)
        .min_w_0()
        .p(px(0.))
        .rounded(px(10.))
        .h(px(44.))
        .flex_shrink_0()
        .when(!enabled && !spinning, |button| button.opacity(0.45))
}

fn toolbar_divider(cx: &App) -> Div {
    let p = palette(cx);
    div()
        .w(px(1.))
        .h(px(22.))
        .mx(px(3.))
        .bg(rgb(p.chrome_border))
}

pub(super) fn toolbar_tile(
    id: &'static str,
    label_key: &'static str,
    glyph: &'static str,
    hint_key: &'static str,
    enabled: bool,
    spinning: bool,
    cx: &App,
) -> Button {
    let p = palette(cx);
    toolbar_button(id, hint_key, enabled, spinning, cx)
        .min_w(px(52.))
        .px(px(6.))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .child(if spinning {
                    div()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            ProgressCircle::new(SharedString::from(format!(
                                "toolbar-spinner-{id}"
                            )))
                            .loading(true)
                            .color(rgb(p.accent)),
                        )
                        .into_any_element()
                } else {
                    icon(glyph, p.chrome_text)
                        .size(px(16.))
                        .into_any_element()
                })
                .child(
                    div()
                        .text_size(px(11.))
                        .line_height(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(p.chrome_text))
                        .when(!spinning, |label| label.opacity(0.86))
                        .child(tr!(label_key)),
                ),
        )
}
