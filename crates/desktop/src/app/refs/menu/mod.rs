mod commands;
mod delete;

use std::rc::Rc;

use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{RefKind, RefOperation, RefRequest, Reference};

use crate::{
    app::{
        Client,
        widgets::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem, menu_style},
    },
    i18n::tr,
};

type OpenChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;

fn delete_label(reference: &Reference) -> String {
    tr!("app.ref_menu.delete", name = &reference.name)
}

#[derive(Clone)]
pub(in crate::app) struct RefMenu {
    id: u64,
    owner: WeakEntity<Client>,
    on_open_change: Option<OpenChange>,
}

impl RefMenu {
    pub fn new(id: u64, owner: WeakEntity<Client>) -> Self {
        Self {
            id,
            owner,
            on_open_change: None,
        }
    }

    pub fn on_open_change(
        mut self,
        callback: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(callback));
        self
    }

    pub fn target(
        &self,
        element: Stateful<Div>,
        reference: Option<&Reference>,
        related_remotes: &[Reference],
    ) -> AnyElement {
        let Some(reference) = reference else {
            return element.into_any_element();
        };
        let menu = self.clone();
        let reference = reference.clone();
        let related_remotes = related_remotes.to_vec();
        element
            .context_menu(move |popup, window, cx| {
                menu.build(popup, &reference, &related_remotes, window, cx)
            })
            .into_any_element()
    }

    fn build(
        &self,
        menu: PopupMenu,
        reference: &Reference,
        related_remotes: &[Reference],
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        if let Some(callback) = &self.on_open_change {
            callback(true, window, cx);
            let callback = callback.clone();
            cx.subscribe_in(
                &cx.entity(),
                window,
                move |_, _, _: &DismissEvent, window, cx| {
                    callback(false, window, cx);
                },
            )
            .detach();
        }
        let menu = menu_style(menu, window).min_w(px(220.)).max_w(px(540.));
        let mut menu = self.items(menu, reference, window, cx);
        for reference in related_remotes.iter().cloned() {
            let source = self.clone();
            menu = menu.submenu(
                reference.name.clone(),
                window,
                cx,
                move |menu, window, cx| {
                    source.items(menu_style(menu, window), &reference, window, cx)
                },
            );
        }
        menu
    }

    fn items(
        &self,
        mut menu: PopupMenu,
        reference: &Reference,
        _: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let Some(owner) = self.owner.upgrade() else {
            return menu;
        };
        let Some(tab) = owner.read(cx).tab(self.id) else {
            return menu;
        };
        let snapshot = &tab.history.data.snapshot;
        let current = snapshot.is_current_branch(reference);
        let other_worktree = reference.kind == RefKind::Local && reference.checked_out && !current;
        let reason = if tab.busy {
            Some(tr!("app.commit_menu.busy"))
        } else if snapshot.sequence.is_some() {
            Some(tr!("app.commit_menu.in_progress"))
        } else if tab.rebase.is_some() {
            Some(tr!("app.rebase.planning_active"))
        } else if other_worktree {
            Some(tr!("app.ref_menu.in_use"))
        } else {
            None
        };
        let kind_label = match reference.kind {
            RefKind::Local => "app.ref_menu.local",
            RefKind::Remote => "app.ref_menu.remote",
            RefKind::Tag => "app.ref_menu.tag",
        };
        menu = menu.label(tr!(kind_label, name = &reference.name));
        let request = |operation| RefRequest::new(snapshot, reference.clone(), operation);
        if current {
            menu = menu.item(
                PopupMenuItem::new(tr!("app.ref_menu.current"))
                    .checked(true)
                    .disabled(true),
            );
        } else {
            let label = match reference.kind {
                RefKind::Remote => "app.ref_menu.track",
                RefKind::Local => "app.ref_menu.checkout",
                RefKind::Tag => "app.ref_menu.checkout_tag",
            };
            menu = menu.item(self.operation_item(
                tr!(label),
                RefRequest::checkout(snapshot, reference.clone()),
                reason.clone(),
            ));
        }
        if reference.kind != RefKind::Remote {
            menu = menu.item(self.operation_item(
                tr!("app.ref_menu.rename"),
                request(RefOperation::Rename(reference.name.clone())),
                reason.clone(),
            ));
        }
        let delete_reason = if current {
            Some(tr!("app.ref_menu.in_use"))
        } else {
            reason
        };
        menu = menu.item(self.operation_item(
            delete_label(reference),
            request(RefOperation::Delete),
            delete_reason,
        ));
        let name = reference.name.clone();
        let hash = reference.id.clone();
        menu.separator()
            .item(
                PopupMenuItem::new(tr!("app.ref_menu.copy_name")).on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
                }),
            )
            .item(
                PopupMenuItem::new(tr!("app.commit_menu.copy_hash")).on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(hash.clone()));
                }),
            )
    }

    fn operation_item(
        &self,
        label: String,
        request: RefRequest,
        reason: Option<String>,
    ) -> PopupMenuItem {
        let source = self.clone();
        let disabled = reason.is_some();
        PopupMenuItem::element(move |_, _| {
            div()
                .id(SharedString::from(format!("reference-menu-{label}")))
                .w_full()
                .child(label.clone())
                .when_some(reason.clone(), |item, reason| {
                    item.tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                })
        })
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            let _ = source.owner.update(cx, |client, cx| {
                client.run_ref_action(source.id, request.clone(), window, cx)
            });
        })
    }
}
