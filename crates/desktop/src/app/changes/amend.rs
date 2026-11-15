use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{CommitAction, CommitAvailability, CommitContext};

use crate::app::{Client, Notice, RepoTab};

pub(super) struct AmendDraft {
    pub(super) context: Arc<CommitContext>,
    summary: String,
    description: String,
}

impl RepoTab {
    pub(super) fn amend_enabled(&self) -> bool {
        let history = &self.history.data;
        history
            .snapshot
            .head
            .as_ref()
            .and_then(|head| history.position.get(head))
            .and_then(|&index| history.snapshot.commits.get(index))
            .and_then(|commit| history.menu.state(&history.snapshot, commit))
            .is_some_and(|state| {
                state.availability(CommitAction::Amend) == CommitAvailability::Enabled
            })
    }
}

impl Client {
    pub(super) fn toggle_worktree_amend(
        &mut self,
        id: u64,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        if !checked {
            self.restore_commit_draft(id, window, cx);
            return;
        }
        if tab.changes.amend.is_some() || !tab.amend_enabled() {
            return;
        }
        let Some(head) = tab.history.data.snapshot.head.clone() else {
            return;
        };
        let repo = tab.repo.clone();
        tab.busy = true;
        Self::spawn_background(
            window,
            cx,
            move || {
                let context = repo.commit_context(&head)?;
                context.ensure_available(CommitAction::Amend)?;
                Ok::<_, ritkraken_git::Error>(Arc::new(context))
            },
            move |this, result, window, cx| {
                let Some(tab) = this.tab_mut(id) else { return };
                tab.busy = false;
                match result {
                    Ok(context) => {
                        let (summary, description) = context
                            .message
                            .split_once('\n')
                            .map_or((context.message.as_str(), ""), |(summary, rest)| {
                                (summary, rest.trim_start_matches('\n'))
                            });
                        let summary = summary.to_owned();
                        let description = description.to_owned();
                        tab.changes.amend = Some(AmendDraft {
                            context,
                            summary: tab.changes.summary.read(cx).value().to_string(),
                            description: tab.changes.description.read(cx).value().to_string(),
                        });
                        tab.changes
                            .summary
                            .update(cx, |input, cx| input.set_value(summary, window, cx));
                        tab.changes
                            .description
                            .update(cx, |input, cx| input.set_value(description, window, cx));
                    }
                    Err(error) => tab.notify(Notice::failure(error), window, cx),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(in crate::app) fn restore_commit_draft(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(draft) = tab.changes.amend.take() else {
            return;
        };
        tab.changes
            .summary
            .update(cx, |input, cx| input.set_value(draft.summary, window, cx));
        tab.changes.description.update(cx, |input, cx| {
            input.set_value(draft.description, window, cx)
        });
        cx.notify();
    }
}
