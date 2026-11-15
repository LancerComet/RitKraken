use std::sync::Arc;

use gpui_kit::{prelude::*, *};
use ritkraken_git::{Change, FileTarget, IgnoreRule, Scope};

use crate::{
    app::{
        Client,
        widgets::menu::{PopupMenu, PopupMenuItem, menu_style},
    },
    i18n::tr,
};

use super::{StageAction, StageTarget, inspect::InspectionKind};

#[derive(Clone)]
pub(super) struct FileMenu {
    pub id: u64,
    pub file: FileTarget,
    pub view: WeakEntity<Client>,
    pub files: Arc<Vec<Change>>,
}

#[derive(Clone, Copy)]
pub(super) enum FileAction {
    Stage,
    Discard,
    Ignore(IgnoreRule, bool),
    Stash,
    Inspect(InspectionKind),
    Open,
    Reveal,
    CopyPath,
    ExportPatch,
}

impl FileMenu {
    pub(super) fn build(
        &self,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let Some(client) = self.view.upgrade() else {
            return menu;
        };
        let selected = client.update(cx, |this, cx| {
            let files = this.selected_file_targets(self.id, &self.file, &self.files);
            cx.notify();
            files
        });
        if selected.is_empty() {
            return menu;
        }
        let multiple = selected.len() > 1;
        let conflicted = selected.iter().any(|file| file.change.code == b'U');
        let Some(tab) = client.read(cx).tab(self.id) else {
            return menu;
        };
        let enabled = !tab.busy;
        let has_head = tab.history.data.snapshot.head.is_some();
        let committed = matches!(self.file.scope, Scope::Commit { .. });
        let existing = tab.repo.root().join(&self.file.change.path).exists();
        let staged_addition = tab
            .history
            .data
            .snapshot
            .status
            .staged
            .iter()
            .any(|change| change.path == self.file.change.path && change.code == b'A');
        let action = StageAction::for_scope(&self.file.scope);
        let ignore = self.clone();
        let stop_tracking = self.clone();
        let tracked = self.file.scope == Scope::Index || self.file.change.code != b'?';
        menu_style(menu, window)
            .min_w(px(230.))
            .when(multiple, |menu| {
                menu.item(
                    PopupMenuItem::new(tr!("app.file_menu.selection", count = selected.len()))
                        .disabled(true),
                )
                .separator()
            })
            .when_some(action, |menu, action| {
                menu.item(self.item(
                    if conflicted {
                        tr!("app.conflicts.resolve")
                    } else {
                        action.label(StageTarget::Row)
                    },
                    FileAction::Stage,
                    enabled,
                ))
                .item(self.item(
                    tr!("app.file_menu.discard"),
                    FileAction::Discard,
                    enabled && !conflicted,
                ))
                .when(!multiple, |menu| {
                    menu.submenu(
                        tr!("app.file_menu.ignore"),
                        window,
                        cx,
                        move |menu, window, _| ignore.ignore_menu(menu, false, enabled, window),
                    )
                    .when(tracked, |menu| {
                        menu.submenu(
                            tr!("app.file_menu.ignore_stop_tracking"),
                            window,
                            cx,
                            move |menu, window, _| {
                                stop_tracking.ignore_menu(menu, true, enabled, window)
                            },
                        )
                    })
                    .item(self.item(
                        tr!("app.file_menu.stash"),
                        FileAction::Stash,
                        enabled && has_head && self.file.change.code != b'U',
                    ))
                })
            })
            .when(!multiple, |menu| {
                menu.separator()
                    .item(self.item(
                        tr!("app.file_menu.history"),
                        FileAction::Inspect(InspectionKind::History),
                        enabled && (committed || has_head),
                    ))
                    .item(self.item(
                        tr!("app.file_menu.blame"),
                        FileAction::Inspect(InspectionKind::Blame),
                        enabled
                            && if committed {
                                self.file.change.code != b'D'
                            } else {
                                has_head
                                    && existing
                                    && !staged_addition
                                    && !matches!(self.file.change.code, b'?' | b'A' | b'U')
                            },
                    ))
                    .separator()
                    .item(self.item(
                        tr!("app.file_menu.open"),
                        FileAction::Open,
                        if committed {
                            self.file.change.code != b'D'
                        } else {
                            existing
                        },
                    ))
                    .item(self.item(tr!("app.file_menu.reveal"), FileAction::Reveal, true))
            })
            .separator()
            .item(self.item(tr!("app.file_menu.copy_path"), FileAction::CopyPath, true))
            .item(self.item(
                tr!("app.file_menu.export_patch"),
                FileAction::ExportPatch,
                enabled && !conflicted,
            ))
    }

    fn item(&self, label: String, action: FileAction, enabled: bool) -> PopupMenuItem {
        let target = self.clone();
        PopupMenuItem::new(label)
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                let file = target.file.clone();
                let _ = target.view.update(cx, |this, cx| {
                    let files = this.selected_file_targets(target.id, &file, &target.files);
                    this.run_file_actions(target.id, files, action, window, cx)
                });
            })
    }

    fn ignore_menu(
        &self,
        menu: PopupMenu,
        stop_tracking: bool,
        enabled: bool,
        window: &Window,
    ) -> PopupMenu {
        let path = &self.file.change.path;
        menu_style(menu, window)
            .item(self.item(
                tr!(
                    "app.file_menu.ignore_file",
                    name = path.file_name().unwrap_or_default().to_string_lossy()
                ),
                FileAction::Ignore(IgnoreRule::File, stop_tracking),
                enabled,
            ))
            .when_some(path.extension(), |menu, extension| {
                menu.item(self.item(
                    tr!(
                        "app.file_menu.ignore_extension",
                        extension = extension.to_string_lossy()
                    ),
                    FileAction::Ignore(IgnoreRule::Extension, stop_tracking),
                    enabled,
                ))
            })
    }
}
