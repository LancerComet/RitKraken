use gpui_kit::*;
use ritkraken_git::Stash;

use crate::{
    app::{
        Client, RepoOperation,
        widgets::{
            delete_dialog::DeleteDialog,
            menu::{PopupMenu, PopupMenuItem, menu_style},
        },
    },
    i18n::tr,
};

/// Right-click menu of a stash row in the commit graph.
pub(super) fn stash_menu(
    id: u64,
    stash: &Stash,
    owner: &WeakEntity<Client>,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let available = owner.upgrade().is_some_and(|client| {
        client.read(cx).tab(id).is_some_and(|tab| {
            !tab.busy && tab.rebase.is_none() && tab.history.data.snapshot.sequence.is_none()
        })
    });
    let menu = menu_style(menu, window)
        .min_w(px(220.))
        .label(tr!("app.stash.label", name = stash.selector()));
    let apply = {
        let (owner, stash) = (owner.clone(), stash.clone());
        PopupMenuItem::new(tr!("app.stash.apply"))
            .disabled(!available)
            .on_click(move |_, window, cx| {
                let operation = RepoOperation::ApplyStash(stash.clone());
                let _ = owner.update(cx, |client, cx| {
                    client.update_repo(id, Some(operation), window, cx)
                });
            })
    };
    let delete = {
        let (owner, stash) = (owner.clone(), stash.clone());
        PopupMenuItem::new(tr!("app.stash.delete"))
            .disabled(!available)
            .on_click(move |_, window, cx| {
                let stash = stash.clone();
                let _ = owner.update(cx, |_, cx| {
                    DeleteDialog::open(
                        id,
                        tr!("app.stash.confirm_delete_title"),
                        tr!("app.stash.confirm_delete", name = stash.selector()),
                        RepoOperation::DropStash(stash),
                        window,
                        cx,
                    )
                });
            })
    };
    let hash = stash.id.clone();
    menu.item(apply).item(delete).separator().item(
        PopupMenuItem::new(tr!("app.commit_menu.copy_hash")).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(hash.clone()));
        }),
    )
}
