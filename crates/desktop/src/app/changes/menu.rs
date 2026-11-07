use std::sync::Arc;

use gpui_kit::{prelude::*, *};
use ritkraken_git::{FileTarget, IgnoreRule, Scope};

use crate::{
    app::{
        Client, Message, RepoOperation,
        widgets::menu::{PopupMenu, PopupMenuItem},
    },
    i18n::tr,
};

use super::{
    StageAction, StageTarget,
    inspect::{FileInspection, InspectionKind},
};

#[derive(Clone)]
pub(super) struct FileMenu {
    pub id: u64,
    pub file: FileTarget,
    pub view: WeakEntity<Client>,
}

#[derive(Clone, Copy)]
enum FileAction {
    Stage,
    Discard,
    Ignore(IgnoreRule),
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
        let filename = self
            .file
            .change
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let extension = self
            .file
            .change
            .path
            .extension()
            .map(|value| value.to_string_lossy().into_owned());
        menu_style(menu, window)
            .min_w(px(230.))
            .when_some(action, |menu, action| {
                menu.item(self.item(action.label(StageTarget::Row), FileAction::Stage, enabled))
                    .item(self.item(
                        tr!("app.file_menu.discard"),
                        FileAction::Discard,
                        enabled && self.file.change.code != b'U',
                    ))
                    .submenu(
                        tr!("app.file_menu.ignore"),
                        window,
                        cx,
                        move |menu, window, _| {
                            menu_style(menu, window)
                                .item(ignore.item(
                                    tr!("app.file_menu.ignore_file", name = filename),
                                    FileAction::Ignore(IgnoreRule::File),
                                    enabled,
                                ))
                                .when_some(extension.as_ref(), |menu, extension| {
                                    menu.item(ignore.item(
                                        tr!(
                                            "app.file_menu.ignore_extension",
                                            extension = extension
                                        ),
                                        FileAction::Ignore(IgnoreRule::Extension),
                                        enabled,
                                    ))
                                })
                        },
                    )
                    .item(self.item(
                        tr!("app.file_menu.stash"),
                        FileAction::Stash,
                        enabled && has_head && self.file.change.code != b'U',
                    ))
            })
            .separator()
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
            .separator()
            .item(self.item(tr!("app.file_menu.copy_path"), FileAction::CopyPath, true))
            .item(self.item(
                tr!("app.file_menu.export_patch"),
                FileAction::ExportPatch,
                enabled && self.file.change.code != b'U',
            ))
    }

    fn item(&self, label: String, action: FileAction, enabled: bool) -> PopupMenuItem {
        let target = self.clone();
        PopupMenuItem::new(label)
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                let file = target.file.clone();
                let _ = target.view.update(cx, |this, cx| {
                    this.run_file_action(target.id, file, action, window, cx)
                });
            })
    }
}

fn menu_style(menu: PopupMenu, window: &Window) -> PopupMenu {
    menu.text_size(rems(0.875).to_pixels(window.rem_size()) - px(2.))
        .padding(rems(0.25).to_pixels(window.rem_size()) + px(2.))
}

impl Client {
    fn run_file_action(
        &mut self,
        id: u64,
        file: FileTarget,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let path = tab.repo.root().join(&file.change.path);
        match action {
            FileAction::Open => self.open_file(id, file, window, cx),
            FileAction::Reveal => self.show_file_in_folder(id, path, window, cx),
            FileAction::CopyPath => cx.write_to_clipboard(ClipboardItem::new_string(
                path.to_string_lossy().into_owned(),
            )),
            _ if tab.busy => {}
            FileAction::Stage => {
                if let Some(stage) = StageAction::for_scope(&file.scope) {
                    self.update_repo(
                        id,
                        Some(RepoOperation::StageFiles(stage, Some(file.change))),
                        window,
                        cx,
                    );
                }
            }
            FileAction::Discard => self.confirm_discard_file(id, file, window, cx),
            FileAction::Ignore(rule) => self.confirm_ignore_file(id, file, rule, window, cx),
            FileAction::Stash => {
                self.update_repo(id, Some(RepoOperation::StashFile(file.change)), window, cx)
            }
            FileAction::Inspect(kind) => {
                FileInspection::open(
                    tab.repo.clone(),
                    file,
                    kind,
                    self.saved.preferences.editor.diff_typography(),
                    window,
                    cx,
                );
            }
            FileAction::ExportPatch => self.export_file_patch(id, file, window, cx),
        }
    }

    fn confirm_discard_file(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        tab.busy = true;
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || repo.prepare_discard(&file).map(|expected| (expected, file)),
            move |this, result, window, cx| {
                let Some(tab) = this.tab_mut(id) else { return };
                tab.busy = false;
                let (expected, file) = match result {
                    Ok(value) => value,
                    Err(error) => {
                        tab.notice = Some((true, error.into()));
                        cx.notify();
                        return;
                    }
                };
                let detail = tr!(
                    match file.scope {
                        Scope::Index => "app.file_menu.discard_staged_detail",
                        _ if file.change.code == b'?' => "app.file_menu.discard_untracked_detail",
                        _ => "app.file_menu.discard_detail",
                    },
                    path = file.change.path.display()
                );
                let prompt = window.prompt(
                    PromptLevel::Warning,
                    &tr!("app.file_menu.discard_title"),
                    Some(&detail),
                    &[
                        PromptButton::cancel(tr!("app.common.cancel")),
                        PromptButton::ok(tr!("app.file_menu.discard")),
                    ],
                    cx,
                );
                cx.spawn_in(window, async move |view, cx| {
                    if prompt.await == Ok(1) {
                        let _ = view.update_in(cx, |this, window, cx| {
                            this.update_repo(
                                id,
                                Some(RepoOperation::DiscardFile(Arc::new(expected))),
                                window,
                                cx,
                            )
                        });
                    }
                })
                .detach();
                cx.notify();
            },
        );
        cx.notify();
    }

    fn confirm_ignore_file(
        &mut self,
        id: u64,
        file: FileTarget,
        rule: IgnoreRule,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if file.change.code == b'?' && file.scope == Scope::Worktree {
            self.update_repo(
                id,
                Some(RepoOperation::IgnoreFile(file.change, rule, false)),
                window,
                cx,
            );
            return;
        }
        let detail = tr!(
            "app.file_menu.ignore_tracked_detail",
            path = file.change.path.display()
        );
        let prompt = window.prompt(
            PromptLevel::Info,
            &tr!("app.file_menu.ignore"),
            Some(&detail),
            &[
                PromptButton::cancel(tr!("app.common.cancel")),
                PromptButton::ok(tr!("app.file_menu.ignore")),
                PromptButton::new(tr!("app.file_menu.ignore_stop_tracking")),
            ],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(answer @ (1 | 2)) = prompt.await {
                let _ = view.update_in(cx, |this, window, cx| {
                    this.update_repo(
                        id,
                        Some(RepoOperation::IgnoreFile(file.change, rule, answer == 2)),
                        window,
                        cx,
                    )
                });
            }
        })
        .detach();
    }

    fn export_file_patch(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || repo.export_patch(&file).map(|bytes| (bytes, repo, file)),
            move |this, result, window, cx| {
                let (bytes, repo, file) = match result {
                    Ok(value) => value,
                    Err(error) => {
                        this.file_action_error(id, error.into(), cx);
                        return;
                    }
                };
                let Some(tab) = this.tab_mut(id) else { return };
                if bytes.is_empty() {
                    tab.notice = Some((false, Message::Key("app.review.no_text_changes")));
                    cx.notify();
                    return;
                }
                let name = format!(
                    "{}.patch",
                    file.change
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                );
                let prompt = cx.prompt_for_new_path(repo.root(), Some(&name));
                cx.spawn_in(window, async move |view, cx| {
                    let result = match prompt.await {
                        Ok(Ok(Some(path))) => {
                            cx.background_executor()
                                .spawn(async move {
                                    std::fs::write(path, bytes).map_err(|error| {
                                        Message::detail("app.file_menu.export_failed", error)
                                    })
                                })
                                .await
                        }
                        Ok(Err(error)) => {
                            Err(Message::detail("app.file_menu.export_failed", error))
                        }
                        _ => return,
                    };
                    let _ = view.update_in(cx, |this, _, cx| {
                        if let Some(tab) = this.tab_mut(id) {
                            tab.notice = Some(match result {
                                Ok(()) => (false, Message::Key("app.file_menu.patch_saved")),
                                Err(error) => (true, error),
                            });
                            cx.notify();
                        }
                    });
                })
                .detach();
            },
        );
    }

    fn file_action_error(&mut self, id: u64, message: Message, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab_mut(id) {
            tab.notice = Some((true, message));
            cx.notify();
        }
    }
}
