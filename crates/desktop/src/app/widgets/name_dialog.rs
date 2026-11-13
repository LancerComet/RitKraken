use std::rc::Rc;

use gpui_kit::{
    component::{
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
    },
    prelude::*,
    *,
};

use super::{Tone, button, note};
use crate::{
    app::{
        Client, Escape, Submit,
        modals::{ContentModal, ModalHandle},
    },
    i18n::tr,
};

type Accept = Rc<dyn Fn(String, bool, &mut Window, &mut App)>;

/// Shared name-entry form for creating and renaming Git references.
pub(in crate::app) struct NameDialog {
    name: Entity<InputState>,
    checkbox: Option<(String, bool)>,
    hint: Option<String>,
    accept: Accept,
    modal: ModalHandle,
    _input: Subscription,
}

impl NameDialog {
    pub fn open(
        title: String,
        initial: &str,
        checkbox: Option<(String, bool)>,
        hint: Option<String>,
        window: &mut Window,
        cx: &mut Context<Client>,
        accept: impl Fn(String, bool, &mut Window, &mut App) + 'static,
    ) {
        let modal = ModalHandle::new(cx);
        let handle = modal.clone();
        let view = cx.new(|cx| {
            let name = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(initial)
                    .placeholder(tr!("app.commit_menu.name"))
            });
            let subscription = cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify());
            Self {
                name,
                checkbox,
                hint,
                accept: Rc::new(accept),
                modal: handle,
                _input: subscription,
            }
        });
        let focus = view.read(cx).name.read(cx).focus_handle(cx);
        Client::present_content_modal(
            ContentModal::new(modal, title, view, size(px(540.), px(650.)), true).focus(focus),
            window,
            cx,
        );
    }

    fn submit(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let checked = self.checkbox.as_ref().is_some_and(|(_, checked)| *checked);
        self.modal.close(window, cx);
        (self.accept)(name, checked, window, cx);
    }
}

impl Render for NameDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("reference-name-dialog")
            .key_context("CommitOperation")
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_size(px(12.))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.submit(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .child(Input::new(&self.name).text_size(px(12.)))
            .when_some(self.checkbox.clone(), |element, (label, checked)| {
                element.child(
                    Checkbox::new("reference-name-checkbox")
                        .label(label)
                        .checked(checked)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            if let Some((_, value)) = &mut this.checkbox {
                                *value = *checked;
                            }
                            cx.notify();
                        })),
                )
            })
            .when_some(self.hint.clone(), |element, hint| element.child(note(hint, cx)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            "cancel-reference-name",
                            tr!("app.common.cancel"),
                            Tone::Outline,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.modal.close(window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    )
                    .child(
                        button(
                            "apply-reference-name",
                            tr!("app.commit_menu.apply"),
                            Tone::Green,
                            !self.name.read(cx).value().trim().is_empty(),
                            cx,
                            cx.listener(|this, _, window, cx| this.submit(window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    ),
            )
    }
}
