use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::{prelude::*, *};
use ritkraken_git::PullMode;

use super::{
    Client, RepoOperation, RepoTab,
    session::ActiveOp,
    theme::{Palette, palette},
    toolbar::{toolbar_button, toolbar_tile},
    widgets::icon,
};
use crate::i18n::tr;

/// The operation the toolbar's Fetch/Pull button runs when clicked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PullAction {
    #[default]
    FetchAll,
    Pull(PullMode),
}

impl PullAction {
    pub(super) const ALL: [Self; 4] = [
        Self::FetchAll,
        Self::Pull(PullMode::FastForwardOrMerge),
        Self::Pull(PullMode::FastForwardOnly),
        Self::Pull(PullMode::Rebase),
    ];

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::FetchAll => "fetch_all",
            Self::Pull(PullMode::FastForwardOrMerge) => "pull_ff",
            Self::Pull(PullMode::FastForwardOnly) => "pull_ff_only",
            Self::Pull(PullMode::Rebase) => "pull_rebase",
        }
    }

    pub(super) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::FetchAll => "app.toolbar.fetch_all",
            Self::Pull(PullMode::FastForwardOrMerge) => "app.toolbar.pull_ff",
            Self::Pull(PullMode::FastForwardOnly) => "app.toolbar.pull_ff_only",
            Self::Pull(PullMode::Rebase) => "app.toolbar.pull_rebase",
        }
    }

    fn button_key(self) -> &'static str {
        match self {
            Self::FetchAll => "app.toolbar.fetch",
            Self::Pull(_) => "app.toolbar.pull",
        }
    }
}

impl Client {
    /// The Fetch/Pull tile and the dropdown that chooses what it runs.
    pub(super) fn pull_controls(
        &self,
        tab: &RepoTab,
        remote_available: bool,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 2] {
        let id = tab.id;
        let snapshot = &tab.history.data.snapshot;
        let selected = self.saved.preferences.pull_action;
        let operation = match selected {
            PullAction::FetchAll => Some(RepoOperation::Fetch(None)),
            PullAction::Pull(mode) => snapshot
                .branch
                .clone()
                .zip(snapshot.upstream.clone())
                .filter(|(_, upstream)| snapshot.remotes.contains(&upstream.remote))
                .filter(|_| {
                    snapshot
                        .refs
                        .iter()
                        .any(|reference| snapshot.is_current_branch(reference))
                })
                .map(|(branch, upstream)| RepoOperation::Pull {
                    branch,
                    upstream,
                    mode,
                }),
        };
        let hint = if operation.is_none() {
            "app.toolbar.pull_no_upstream"
        } else {
            selected.label_key()
        };
        let tile = toolbar_tile(
            "fetch",
            selected.button_key(),
            "icons/arrow-down-to-line.svg",
            hint,
            remote_available && operation.is_some(),
            tab.active_op == Some(ActiveOp::Fetch),
            cx,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            if let Some(operation) = operation.clone() {
                this.update_repo(id, Some(operation), window, cx);
            }
        }));

        let p = palette(cx);
        let remotes = snapshot.remotes.clone();
        let view = cx.weak_entity();
        let menu = toolbar_button(
            "fetch-options",
            "app.toolbar.pull_options",
            remote_available,
            false,
            cx,
        )
        .w(px(20.))
        .rounded(px(8.))
        .child(icon("icons/chevron-down.svg", p.chrome_text).size(px(11.)))
        .dropdown_menu(move |menu, _, cx| {
            let p = palette(cx);
            let menu = menu.min_w(MENU_WIDTH).max_w(MENU_WIDTH).item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .w(MENU_WIDTH - px(40.))
                        .py(px(4.))
                        .text_xs()
                        .text_color(rgb(p.text_muted))
                        .child(tr!("app.toolbar.pull_default_hint"))
                })
                .disabled(true),
            );
            let menu = PullAction::ALL.into_iter().fold(menu, |menu, action| {
                let view = view.clone();
                let label: SharedString = tr!(action.label_key()).into();
                let checked = action == selected;
                menu.item(
                    PopupMenuItem::element(move |_, _| radio_row(label.clone(), checked, p))
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| this.choose_pull_action(action, cx));
                        }),
                )
            });
            remotes.iter().fold(menu.separator(), |menu, remote| {
                let view = view.clone();
                let label: SharedString = tr!("app.toolbar.fetch_remote", remote = remote).into();
                let edit = RepoOperation::Fetch(Some(remote.clone()));
                menu.item(
                    PopupMenuItem::element(move |_, _| div().text_xs().child(label.clone()))
                        .on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.update_repo(id, Some(edit.clone()), window, cx)
                            });
                        }),
                )
            })
        });
        [tile.into_any_element(), menu.into_any_element()]
    }

    fn choose_pull_action(&mut self, action: PullAction, cx: &mut Context<Self>) {
        if self.saved.preferences.pull_action != action {
            self.saved.preferences.pull_action = action;
            self.saved.save();
            cx.notify();
        }
    }
}

// Menu text is one step below the shared menu size (text_sm).
const MENU_WIDTH: Pixels = px(260.);

fn radio_row(label: SharedString, checked: bool, p: Palette) -> Div {
    let ring = if checked { p.accent } else { p.text_muted };
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_xs()
        .child(
            div()
                .size(px(14.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(rgb(ring))
                .when(checked, |radio| {
                    radio.child(div().size(px(8.)).rounded_full().bg(rgb(p.accent)))
                }),
        )
        .child(label)
}
