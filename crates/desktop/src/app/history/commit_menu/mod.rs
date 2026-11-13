mod commands;
mod compare;
mod dialog;
mod links;
mod references;

use std::sync::Arc;

use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{
    self as git, CommitAction, CommitAvailability, CommitBlock, CommitMenuState, CommitOperation,
    CommitRelation, RefKind, ResetMode,
};

use crate::{
    app::{
        Client,
        widgets::menu::{PopupMenu, PopupMenuItem, menu_style},
    },
    i18n::tr,
};

#[derive(Clone)]
pub(in crate::app::history) struct CommitMenu {
    pub id: u64,
    pub target: String,
    pub owner: WeakEntity<Client>,
}

#[derive(Clone)]
enum GraphAction {
    Operation(CommitOperation),
    Compare,
    CopyHash,
    CopyMessage,
    Export(Option<usize>),
    OpenRemote(String),
}

impl CommitMenu {
    pub fn build(
        &self,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let Some(owner) = self.owner.upgrade() else {
            return menu;
        };
        let Some(tab) = owner.read(cx).tab(self.id) else {
            return menu;
        };
        let history = &tab.history.data;
        let context = history
            .position
            .get(&self.target)
            .and_then(|&index| history.snapshot.commits.get(index))
            .and_then(|commit| history.menu.state(&history.snapshot, commit))
            .map(Arc::new);
        if let Some(context) = context {
            self.items(menu, context, window, cx)
        } else {
            menu_style(menu, window).item(self.read_item(
                tr!("app.commit_menu.copy_hash"),
                GraphAction::CopyHash,
                None,
            ))
        }
    }

    fn items(
        &self,
        menu: PopupMenu,
        context: Arc<CommitMenuState>,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let busy = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).tab(self.id).map(|tab| tab.busy))
            .unwrap_or(true);
        let branch = context.branch.clone().unwrap_or_else(|| "HEAD".to_owned());
        let mut menu = menu_style(menu, window)
            .min_w(px(250.))
            .max_w(px(540.))
            .scrollable(true)
            .item(self.operation_item(
                tr!("app.commit_menu.create_branch"),
                &context,
                CommitOperation::CreateBranch {
                    name: String::new(),
                    checkout: false,
                    track: None,
                },
                busy,
            ))
            .item(self.operation_item(
                tr!("app.commit_menu.create_tag"),
                &context,
                CommitOperation::CreateTag(String::new()),
                busy,
            ));
        let branches: Vec<_> = context.checkout_references().cloned().collect();
        if !branches.is_empty() {
            let source = self.clone();
            let context = context.clone();
            menu = menu.submenu(
                tr!("app.commit_menu.switch_branch"),
                window,
                cx,
                move |menu, window, _| {
                    branches
                        .iter()
                        .fold(menu_style(menu, window), |menu, reference| {
                            let operation = if reference.kind == RefKind::Local {
                                CommitOperation::Switch(reference.name.clone())
                            } else {
                                CommitOperation::CreateBranch {
                                    name: context.remote_branch_name(&reference.name).to_owned(),
                                    checkout: true,
                                    track: Some(reference.name.clone()),
                                }
                            };
                            menu.item(source.operation_item(
                                reference.name.clone(),
                                &context,
                                operation,
                                busy,
                            ))
                        })
                },
            );
        }
        if context.relation != CommitRelation::Head {
            menu = menu.item(self.operation_item(
                tr!("app.commit_menu.checkout"),
                &context,
                CommitOperation::Checkout,
                busy,
            ));
        }
        menu = menu.separator();
        if context.relation != CommitRelation::Head {
            menu = menu.item(self.read_item(
                tr!("app.commit_menu.compare"),
                GraphAction::Compare,
                Some(context.clone()),
            ));
        }
        for (action, label, operation) in [
            (
                CommitAction::CherryPick,
                tr!("app.commit_menu.cherry_pick", branch = &branch),
                CommitOperation::CherryPick { mainline: None },
            ),
            (
                CommitAction::Revert,
                tr!("app.commit_menu.revert"),
                CommitOperation::Revert { mainline: None },
            ),
        ] {
            if context.availability(action) == CommitAvailability::Hidden {
                continue;
            }
            if context.commit.parents.len() > 1 {
                let source = self.clone();
                let context = context.clone();
                menu = menu.submenu(label, window, cx, move |menu, window, _| {
                    context.commit.parents.iter().enumerate().fold(
                        menu_style(menu, window),
                        |menu, (index, parent)| {
                            let operation = if action == CommitAction::CherryPick {
                                CommitOperation::CherryPick {
                                    mainline: Some(index + 1),
                                }
                            } else {
                                CommitOperation::Revert {
                                    mainline: Some(index + 1),
                                }
                            };
                            menu.item(source.operation_item(
                                tr!(
                                    "app.commit_menu.parent",
                                    number = index + 1,
                                    sha = git::short_id(parent)
                                ),
                                &context,
                                operation,
                                busy,
                            ))
                        },
                    )
                });
            } else {
                menu = menu.item(self.operation_item(label, &context, operation, busy));
            }
        }
        for (label, operation) in [
            (
                if context.relation == CommitRelation::Descendant {
                    tr!("app.commit_menu.fast_forward", branch = &branch)
                } else {
                    tr!("app.commit_menu.merge", branch = &branch)
                },
                CommitOperation::Merge,
            ),
            (
                tr!("app.commit_menu.rebase", branch = &branch),
                CommitOperation::Rebase,
            ),
            (
                tr!("app.commit_menu.interactive"),
                CommitOperation::InteractiveRebase(git::RebasePlan::placeholder(
                    context.commit.id.clone(),
                )),
            ),
            (
                tr!("app.commit_menu.amend"),
                CommitOperation::Amend {
                    message: String::new(),
                    include_staged: false,
                },
            ),
        ] {
            if context.availability(operation.action()) != CommitAvailability::Hidden {
                menu = menu.item(self.operation_item(label, &context, operation, busy));
            }
        }
        if context.availability(CommitAction::Reset) != CommitAvailability::Hidden {
            let source = self.clone();
            let context = context.clone();
            menu = menu.submenu(
                tr!("app.commit_menu.reset", branch = &branch),
                window,
                cx,
                move |menu, window, _| {
                    [ResetMode::Soft, ResetMode::Mixed, ResetMode::Hard]
                        .into_iter()
                        .fold(menu_style(menu, window), |menu, mode| {
                            menu.item(source.operation_item(
                                reset_label(mode),
                                &context,
                                CommitOperation::Reset(mode),
                                busy,
                            ))
                        })
                },
            );
        }
        if context.sequence.is_some() {
            menu = menu.separator();
            for (key, operation) in [
                ("app.commit_menu.continue", CommitOperation::Continue),
                ("app.commit_menu.skip", CommitOperation::Skip),
                ("app.commit_menu.abort", CommitOperation::Abort),
            ] {
                if context.availability(operation.action()) != CommitAvailability::Hidden {
                    menu = menu.item(self.operation_item(tr!(key), &context, operation, busy));
                }
            }
        }
        menu = menu.separator();
        if context.commit.parents.len() > 1 {
            let source = self.clone();
            let context = context.clone();
            menu = menu.submenu(
                tr!("app.commit_menu.export"),
                window,
                cx,
                move |menu, window, _| {
                    context.commit.parents.iter().enumerate().fold(
                        menu_style(menu, window),
                        |menu, (index, parent)| {
                            menu.item(source.read_item(
                                tr!(
                                    "app.commit_menu.parent",
                                    number = index + 1,
                                    sha = git::short_id(parent)
                                ),
                                GraphAction::Export(Some(index + 1)),
                                Some(context.clone()),
                            ))
                        },
                    )
                },
            );
        } else {
            menu = menu.item(self.read_item(
                tr!("app.commit_menu.export"),
                GraphAction::Export(None),
                Some(context.clone()),
            ));
        }
        menu = menu
            .item(self.read_item(
                tr!("app.commit_menu.copy_hash"),
                GraphAction::CopyHash,
                Some(context.clone()),
            ))
            .item(self.read_item(
                tr!("app.commit_menu.copy_message"),
                GraphAction::CopyMessage,
                Some(context.clone()),
            ));
        if let Some(url) = context
            .remote_url
            .as_deref()
            .and_then(|remote| links::commit_url(remote, &context.commit.id))
        {
            menu = menu.item(self.read_item(
                tr!("app.commit_menu.open_remote"),
                GraphAction::OpenRemote(url),
                Some(context),
            ));
        }
        menu
    }

    fn operation_item(
        &self,
        label: String,
        context: &Arc<CommitMenuState>,
        operation: CommitOperation,
        busy: bool,
    ) -> PopupMenuItem {
        let reason = if busy {
            Some(tr!("app.commit_menu.busy"))
        } else {
            match context.availability(operation.action()) {
                CommitAvailability::Enabled => None,
                CommitAvailability::Hidden => Some(tr!("app.commit_menu.unavailable")),
                CommitAvailability::Disabled(block) => Some(tr!(match block {
                    CommitBlock::Dirty => "app.commit_menu.dirty",
                    CommitBlock::Conflict => "app.commit_menu.conflict",
                    CommitBlock::InProgress => "app.commit_menu.in_progress",
                })),
            }
        };
        self.item(
            label,
            GraphAction::Operation(operation),
            Some(context.clone()),
            reason,
        )
    }

    fn read_item(
        &self,
        label: String,
        action: GraphAction,
        context: Option<Arc<CommitMenuState>>,
    ) -> PopupMenuItem {
        self.item(label, action, context, None)
    }

    fn item(
        &self,
        label: String,
        action: GraphAction,
        context: Option<Arc<CommitMenuState>>,
        reason: Option<String>,
    ) -> PopupMenuItem {
        let source = self.clone();
        let disabled = reason.is_some();
        PopupMenuItem::element(move |_, _| {
            let reason = reason.clone();
            div()
                .id(SharedString::from(format!("commit-menu-{label}")))
                .w_full()
                .child(label.clone())
                .when_some(reason, |element, reason| {
                    element
                        .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                })
        })
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            let _ = source.owner.update(cx, |client, cx| {
                client.run_graph_action(
                    source.id,
                    &source.target,
                    context.clone(),
                    action.clone(),
                    window,
                    cx,
                );
            });
        })
    }
}

fn reset_label(mode: ResetMode) -> String {
    tr!(match mode {
        ResetMode::Soft => "app.commit_menu.reset_soft",
        ResetMode::Mixed => "app.commit_menu.reset_mixed",
        ResetMode::Hard => "app.commit_menu.reset_hard",
    })
}

pub(in crate::app::history) fn worktree_menu(
    id: u64,
    owner: &WeakEntity<Client>,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    use crate::app::{
        RepoOperation,
        changes::{StageAction, StageTarget},
    };
    let Some(client) = owner.upgrade() else {
        return menu;
    };
    let Some(tab) = client.read(cx).tab(id) else {
        return menu;
    };
    let snapshot = &tab.history.data.snapshot;
    let available = !tab.busy && tab.rebase.is_none();
    let stash = available
        && tab.has_changes()
        && snapshot.head.is_some()
        && !snapshot
            .status
            .distinct_changes()
            .any(|change| change.code == b'U');
    let edits = [
        (
            StageAction::Stage.label(StageTarget::All),
            RepoOperation::StageFiles(StageAction::Stage, None),
            available && !snapshot.status.unstaged.is_empty(),
        ),
        (
            StageAction::Unstage.label(StageTarget::All),
            RepoOperation::StageFiles(StageAction::Unstage, None),
            available && !snapshot.status.staged.is_empty(),
        ),
        (tr!("app.toolbar.stash"), RepoOperation::Stash, stash),
    ];
    edits
        .into_iter()
        .fold(menu_style(menu, window), |menu, (label, edit, enabled)| {
            let owner = owner.clone();
            menu.item(PopupMenuItem::new(label).disabled(!enabled).on_click(
                move |_, window, cx| {
                    let _ = owner.update(cx, |client, cx| {
                        client.update_repo(id, Some(edit.clone()), window, cx)
                    });
                },
            ))
        })
}
