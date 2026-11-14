use std::sync::Arc;

use gpui_kit::{
    component::{
        Sizable as _,
        input::{Input, InputEvent, InputState},
    },
    prelude::*,
    *,
};
use ritkraken_git::{CommitMenuState, CommitOperation};

use crate::{
    app::{Client, Message, Notice},
    i18n::LocalizedInput,
};

struct Draft {
    id: u64,
    expected: Arc<CommitMenuState>,
    operation: CommitOperation,
}

pub(in crate::app) struct BranchEditor {
    name: LocalizedInput,
    draft: Option<Draft>,
    waiting: Option<u64>,
    _input: Subscription,
}

impl BranchEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let name = LocalizedInput::new("app.branch.name", cx, |cx| InputState::new(window, cx));
        let subscription = cx.subscribe_in(
            &name,
            window,
            |client: &mut Client, _, event: &InputEvent, window, cx| {
                if matches!(
                    event,
                    InputEvent::PressEnter {
                        secondary: false,
                        shift: false
                    }
                ) {
                    client.submit_inline_branch(window, cx);
                }
            },
        );
        Self {
            name,
            draft: None,
            waiting: None,
            _input: subscription,
        }
    }

    pub fn is_editing(&self) -> bool {
        self.draft.is_some() || self.waiting.is_some()
    }
    pub fn clear(&mut self) {
        self.draft = None;
        self.waiting = None;
    }
    pub fn localize(&self, window: &mut Window, cx: &mut App) {
        self.name.localize(window, cx);
    }

    pub(super) fn for_tab(&self, id: u64) -> Option<(String, Entity<InputState>)> {
        let draft = self.draft.as_ref().filter(|draft| draft.id == id)?;
        Some((draft.expected.commit.id.clone(), (&*self.name).clone()))
    }
}

pub(super) fn field(input: &Entity<InputState>, busy: bool) -> Div {
    div()
        .w_full()
        .min_w_0()
        .pl(px(7.))
        .pr(px(4.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Styled::h(Input::new(input).xsmall(), px(24.))
                .w_full()
                .px(px(6.))
                .py(px(0.))
                .text_size(px(12.))
                .line_height(px(18.))
                .rounded_full()
                .disabled(busy),
        )
}

impl Client {
    pub(in crate::app) fn start_toolbar_branch(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id).filter(|tab| !tab.busy) else {
            return;
        };
        let Some(head) = tab.history.data.snapshot.head.as_ref() else {
            return;
        };
        let context = tab
            .history
            .data
            .position
            .get(head)
            .and_then(|&ix| tab.history.data.snapshot.commits.get(ix))
            .and_then(|commit| {
                tab.history
                    .data
                    .menu
                    .state(&tab.history.data.snapshot, commit)
            });
        if let Some(context) = context {
            self.start_inline_branch(
                id,
                Arc::new(context),
                CommitOperation::CreateBranch {
                    name: String::new(),
                    checkout: true,
                    track: None,
                },
                window,
                cx,
            );
        } else if tab.history.limit.is_some() {
            self.branch_editor.waiting = Some(id);
            self.update_repo_with_limit(id, None, None, window, cx);
        } else if let Some(tab) = self.tab_mut(id) {
            tab.notify(
                Notice::warning(Message::Key("app.error.commit_outside_history")),
                window,
                cx,
            );
            cx.notify();
        }
    }

    pub(in crate::app) fn start_inline_branch(
        &mut self,
        id: u64,
        expected: Arc<CommitMenuState>,
        operation: CommitOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let CommitOperation::CreateBranch { name, .. } = &operation else {
            return;
        };
        if self.modal_is_present() || self.tab(id).is_none_or(|tab| tab.busy) {
            return;
        }
        let target = expected.commit.id.clone();
        self.select(id, Some(target), Some(ScrollStrategy::Center), window, cx);
        self.branch_editor.name.update(cx, |input, cx| {
            input.set_value(name.clone(), window, cx);
            input.focus_handle(cx).focus(window, cx);
        });
        self.branch_editor.draft = Some(Draft {
            id,
            expected,
            operation,
        });
        cx.notify();
    }

    pub(in crate::app) fn submit_inline_branch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.branch_editor.draft.as_ref() else {
            return;
        };
        if self.session.active != Some(draft.id) || self.tab(draft.id).is_none_or(|tab| tab.busy) {
            return;
        }
        let name = self.branch_editor.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let mut operation = draft.operation.clone();
        if let CommitOperation::CreateBranch { name: value, .. } = &mut operation {
            *value = name;
        }
        self.load_graph_operation(
            draft.id,
            draft.expected.clone(),
            operation,
            true,
            window,
            cx,
        );
    }

    pub(in crate::app) fn cancel_inline_branch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.branch_editor.clear();
        if let Some(tab) = self.current() {
            tab.history.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(in crate::app) fn finish_inline_branch(
        &mut self,
        id: u64,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .branch_editor
            .draft
            .as_ref()
            .is_some_and(|draft| draft.id == id)
            && self.branch_editor.name.read(cx).value().trim() == name
        {
            self.cancel_inline_branch(window, cx);
        }
    }

    pub(in crate::app) fn resume_toolbar_branch(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.branch_editor.waiting == Some(id) {
            self.branch_editor.waiting = None;
            self.start_toolbar_branch(id, window, cx);
        }
    }
}
