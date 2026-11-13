mod banner;
mod projection;
mod row;

use std::sync::Arc;

use gpui_kit::{
    component::input::{InputEvent, TextareaState},
    *,
};
use ritkraken_git::{self as git, CommitContext, CommitOperation, RebaseAction};

use super::HistoryState;
use crate::{
    app::{Client, Message},
    i18n::tr,
};

pub(in crate::app) use banner::{rebase_planning_bar, sequence_control_bar};
pub(in crate::app) use projection::{
    PlannedRowMeta, ProjectedRebase, RebaseViewMode, find_preview_row_for_entry, project_history,
    real_oid, rebase_row_id,
};
pub(in crate::app) use row::{
    DraggedRebaseRow, drag_preview, planned_context_menu, planned_message_column,
    planned_message_pane,
};

actions!(
    ritkraken,
    [
        RebasePick,
        RebaseReword,
        RebaseEdit,
        RebaseSquash,
        RebaseFixup,
        RebaseDrop,
        RebaseMoveUp,
        RebaseMoveDown
    ]
);

pub(in crate::app) struct RebaseSession {
    pub(in crate::app) context: Arc<CommitContext>,
    pub(in crate::app) initial_plan: git::RebasePlan,
    pub(in crate::app) plan: git::RebasePlan,
    pub(in crate::app) mode: RebaseViewMode,
    pub(in crate::app) projected: ProjectedRebase,
    pub(in crate::app) message_editor: Entity<TextareaState>,
    pub(in crate::app) editing_entry: Option<usize>,
    _message_sub: Subscription,
}

impl RebaseSession {
    pub(in crate::app) fn meta_for(&self, id: &str) -> Option<&PlannedRowMeta> {
        self.projected.row_meta.get(id)
    }

    pub(in crate::app) fn reproject(&mut self, base_history: &Arc<super::History>) {
        self.projected = project_history(base_history, &self.plan, self.mode);
    }

    pub(in crate::app) fn default_selection(&self) -> Option<String> {
        self.projected
            .projected_tip
            .clone()
            .or_else(|| {
                self.plan
                    .entries
                    .last()
                    .map(|entry| rebase_row_id(&entry.commit.id))
            })
            .or_else(|| Some(self.plan.base.clone()))
    }

    fn is_entry_editable(&self, meta: &PlannedRowMeta) -> bool {
        (meta.action == RebaseAction::Reword && meta.folded_count == 0)
            || (self.mode == RebaseViewMode::TreeEdit && meta.action == RebaseAction::Squash)
    }
}

impl Client {
    pub(in crate::app) fn start_rebase_session(
        &mut self,
        id: u64,
        context: Arc<CommitContext>,
        plan: git::RebasePlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(base_history) = self.tab(id).map(|tab| tab.history.data.clone()) else {
            return;
        };
        if plan.entries.is_empty() {
            if let Some(tab) = self.tab_mut(id) {
                tab.notice = Some((true, Message::Key("app.rebase.empty_plan")));
                cx.notify();
            }
            return;
        }
        let projected = project_history(&base_history, &plan, RebaseViewMode::TreeEdit);
        let message_editor = cx.new(|cx| TextareaState::new(window, cx).rows(6));
        let subscription = cx.subscribe_in(
            &message_editor,
            window,
            move |this: &mut Client, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.on_rebase_message_edited(id, cx);
                }
            },
        );
        let session = RebaseSession {
            context,
            initial_plan: plan.clone(),
            plan,
            mode: RebaseViewMode::TreeEdit,
            projected,
            message_editor,
            editing_entry: None,
            _message_sub: subscription,
        };
        let default_selection = session.default_selection();
        let focus = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            tab.rebase = Some(session);
            tab.notice = None;
            tab.history.focus.clone()
        };
        self.select(id, default_selection, Some(ScrollStrategy::Top), window, cx);
        focus.focus(window, cx);
        cx.notify();
    }

    pub(in crate::app) fn sync_rebase_editor_for_selection(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let update = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            let selected = tab.history.selected.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            let meta = selected.as_deref().and_then(|sel| session.meta_for(sel));
            match meta {
                Some(meta) if session.is_entry_editable(meta) => {
                    let entry_index = meta.entry_index;
                    let text = session.plan.entries[entry_index].message.clone();
                    session.editing_entry = Some(entry_index);
                    Some((session.message_editor.clone(), text))
                }
                _ => {
                    session.editing_entry = None;
                    None
                }
            }
        };
        if let Some((editor, text)) = update {
            editor.update(cx, |input, cx| {
                if input.value() != text {
                    input.set_value(text, window, cx);
                }
            });
        }
    }

    fn on_rebase_message_edited(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        let base_history = tab.history.data.clone();
        let Some(session) = tab.rebase.as_mut() else {
            return;
        };
        let Some(entry_index) = session.editing_entry else {
            return;
        };
        let Some(entry) = session.plan.entries.get_mut(entry_index) else {
            return;
        };
        let edited = session.message_editor.read(cx).value().to_string();
        if entry.message == edited {
            return;
        }
        entry.message = edited;
        session.reproject(&base_history);
        cx.notify();
    }

    pub(in crate::app) fn set_rebase_action(
        &mut self,
        id: u64,
        entry_index: usize,
        action: RebaseAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_selection = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            if tab.busy {
                return;
            }
            let base_history = tab.history.data.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            let Some(entry) = session.plan.entries.get_mut(entry_index) else {
                return;
            };
            entry.action = action;
            session.reproject(&base_history);
            find_preview_row_for_entry(&session.projected.row_meta, entry_index, &session.plan)
                .or_else(|| session.default_selection())
        };
        if self
            .tab(id)
            .is_some_and(|tab| tab.history.selected != target_selection)
        {
            self.select(
                id,
                target_selection,
                Some(ScrollStrategy::Nearest),
                window,
                cx,
            );
        } else {
            self.sync_rebase_editor_for_selection(id, window, cx);
            cx.notify();
        }
    }

    pub(in crate::app) fn enable_rebase_reword(
        &mut self,
        id: u64,
        entry_index: usize,
        focus_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_rebase_action(id, entry_index, RebaseAction::Reword, window, cx);
        if focus_editor
            && let Some(tab) = self.tab(id)
            && let Some(session) = &tab.rebase
        {
            let focus = session.message_editor.read(cx).focus_handle(cx);
            focus.focus(window, cx);
        }
    }

    /// Reorder a planned commit in visual graph space (`-1` = visually up/newer, `+1` = visually down/older).
    pub(in crate::app) fn move_rebase_entry_visual(
        &mut self,
        id: u64,
        entry_index: usize,
        visual_delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_id = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            if tab.busy {
                return;
            }
            let base_history = tab.history.data.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            // Display order is reversed (len - 1 at top, 0 at bottom).
            let plan_delta = -visual_delta;
            let destination = entry_index.saturating_add_signed(plan_delta);
            if entry_index >= session.plan.entries.len()
                || destination >= session.plan.entries.len()
                || destination == entry_index
            {
                return;
            }
            session.plan.entries.swap(entry_index, destination);
            session.reproject(&base_history);
            Some(rebase_row_id(&session.plan.entries[destination].commit.id))
        };
        if let Some(selected_id) = selected_id {
            self.select(
                id,
                Some(selected_id),
                Some(ScrollStrategy::Nearest),
                window,
                cx,
            );
        }
    }

    pub(in crate::app) fn move_rebase_entry_to(
        &mut self,
        id: u64,
        from_entry: usize,
        to_entry: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_id = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            if tab.busy {
                return;
            }
            let base_history = tab.history.data.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            let len = session.plan.entries.len();
            if from_entry >= len || to_entry >= len || from_entry == to_entry {
                return;
            }
            let entry = session.plan.entries.remove(from_entry);
            let moved_id = rebase_row_id(&entry.commit.id);
            session.plan.entries.insert(to_entry, entry);
            session.reproject(&base_history);
            moved_id
        };
        self.select(
            id,
            Some(selected_id),
            Some(ScrollStrategy::Nearest),
            window,
            cx,
        );
    }

    pub(in crate::app) fn apply_rebase_shortcut(
        &mut self,
        id: u64,
        action: RebaseAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry_index = {
            let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
                return;
            };
            let Some(session) = tab.rebase.as_ref() else {
                return;
            };
            if session.mode != RebaseViewMode::TreeEdit {
                return;
            }
            tab.history
                .selected
                .as_deref()
                .and_then(|sel| session.meta_for(sel))
                .map(|meta| meta.entry_index)
        };
        if let Some(entry_index) = entry_index {
            self.enable_or_set_shortcut(id, entry_index, action, window, cx);
        }
    }

    fn enable_or_set_shortcut(
        &mut self,
        id: u64,
        entry_index: usize,
        action: RebaseAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == RebaseAction::Reword {
            self.enable_rebase_reword(id, entry_index, false, window, cx);
        } else {
            self.set_rebase_action(id, entry_index, action, window, cx);
        }
    }

    pub(in crate::app) fn move_selected_rebase_visual(
        &mut self,
        id: u64,
        visual_delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry_index = {
            let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
                return;
            };
            let Some(session) = tab.rebase.as_ref() else {
                return;
            };
            if session.mode != RebaseViewMode::TreeEdit {
                return;
            }
            tab.history
                .selected
                .as_deref()
                .and_then(|sel| session.meta_for(sel))
                .map(|meta| meta.entry_index)
        };
        if let Some(entry_index) = entry_index {
            self.move_rebase_entry_visual(id, entry_index, visual_delta, window, cx);
        }
    }

    pub(in crate::app) fn toggle_rebase_preview(
        &mut self,
        id: u64,
        mode: RebaseViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_selection = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            let base_history = tab.history.data.clone();
            let selected = tab.history.selected.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            if session.mode == mode {
                return;
            }
            let prev_meta = selected
                .as_deref()
                .and_then(|sel| session.meta_for(sel))
                .cloned();
            session.mode = mode;
            session.reproject(&base_history);
            match mode {
                RebaseViewMode::ResultPreview => prev_meta
                    .and_then(|meta| {
                        find_preview_row_for_entry(
                            &session.projected.row_meta,
                            meta.entry_index,
                            &session.plan,
                        )
                    })
                    .or_else(|| session.default_selection()),
                RebaseViewMode::TreeEdit => prev_meta
                    .map(|meta| rebase_row_id(&meta.oid))
                    .filter(|row| session.projected.row_meta.contains_key(row))
                    .or_else(|| session.default_selection()),
            }
        };
        self.select(
            id,
            next_selection,
            Some(ScrollStrategy::Nearest),
            window,
            cx,
        );
    }

    pub(in crate::app) fn reset_rebase(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let default_selection = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            if tab.busy {
                return;
            }
            let base_history = tab.history.data.clone();
            let Some(session) = tab.rebase.as_mut() else {
                return;
            };
            session.plan = session.initial_plan.clone();
            session.mode = RebaseViewMode::TreeEdit;
            session.editing_entry = None;
            session.reproject(&base_history);
            session.default_selection()
        };
        self.select(id, default_selection, Some(ScrollStrategy::Top), window, cx);
    }

    pub(in crate::app) fn cancel_rebase(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial_selection = {
            let Some(tab) = self.tab_mut(id) else {
                return;
            };
            if tab.busy || tab.rebase.is_none() {
                return;
            }
            tab.rebase = None;
            HistoryState::initial_selection(&tab.history.data)
        };
        self.select(id, initial_selection, Some(ScrollStrategy::Top), window, cx);
    }

    pub(in crate::app) fn submit_rebase(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let payload = {
            let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
                return;
            };
            let Some(session) = tab.rebase.as_ref() else {
                return;
            };
            if session.projected.validation_error.is_some() {
                return;
            }
            (session.context.clone(), session.plan.clone())
        };
        let (context, plan) = payload;
        self.prepare_graph_operation(
            id,
            context,
            CommitOperation::InteractiveRebase(plan),
            true,
            window,
            cx,
        );
    }

    pub(in crate::app) fn run_sequence_action(
        &mut self,
        id: u64,
        operation: CommitOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = {
            let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
                return;
            };
            let snapshot = &tab.history.data.snapshot;
            let Some(head_id) = snapshot.head.as_deref() else {
                return;
            };
            let Some(&index) = tab.history.data.position.get(head_id) else {
                return;
            };
            let commit = &snapshot.commits[index];
            let Some(state) = tab.history.data.menu.state(snapshot, commit) else {
                return;
            };
            Arc::new(state)
        };
        let reviewed = matches!(operation, CommitOperation::Continue);
        self.load_graph_operation(id, state, operation, reviewed, window, cx);
    }

    pub(in crate::app) fn amend_paused_rebase(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operation = CommitOperation::Amend {
            message: String::new(),
            include_staged: false,
        };
        self.run_sequence_action(id, operation, window, cx);
    }
}

pub(super) fn action_label(action: RebaseAction) -> String {
    tr!(match action {
        RebaseAction::Pick => "app.commit_menu.pick",
        RebaseAction::Reword => "app.commit_menu.reword",
        RebaseAction::Edit => "app.commit_menu.edit",
        RebaseAction::Squash => "app.commit_menu.squash",
        RebaseAction::Fixup => "app.commit_menu.fixup",
        RebaseAction::Drop => "app.commit_menu.drop",
    })
}
