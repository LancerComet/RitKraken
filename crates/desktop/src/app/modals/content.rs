use std::{cell::Cell, rc::Rc};

use gpui_kit::{prelude::*, *};

use crate::app::{Client, widgets::modal::Modal};

#[derive(Default)]
struct Lifecycle {
    open: Cell<bool>,
    busy: Cell<bool>,
}

/// A view can close only its own modal, including after asynchronous work.
#[derive(Clone)]
pub(in crate::app) struct ModalHandle {
    owner: WeakEntity<Client>,
    state: Rc<Lifecycle>,
}

impl ModalHandle {
    pub fn new(cx: &Context<Client>) -> Self {
        Self {
            owner: cx.weak_entity(),
            state: Rc::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.open.get()
    }
    pub fn is_busy(&self) -> bool {
        self.state.busy.get()
    }
    pub fn set_busy(&self, busy: bool) {
        self.state.busy.set(busy);
    }

    pub fn close(&self, window: &mut Window, cx: &mut App) {
        let _ = self.owner.update(cx, |client, cx| {
            client.close_content_modal(self, window, cx)
        });
    }
}

#[derive(Clone)]
pub(in crate::app) struct ContentModal {
    handle: ModalHandle,
    title: SharedString,
    content: AnyView,
    maximum: Size<Pixels>,
    fit_content: bool,
    initial_focus: Option<FocusHandle>,
}

impl ContentModal {
    pub fn new(
        handle: ModalHandle,
        title: impl Into<SharedString>,
        content: impl Into<AnyView>,
        maximum: Size<Pixels>,
        fit_content: bool,
    ) -> Self {
        Self {
            handle,
            title: title.into(),
            content: content.into(),
            maximum,
            fit_content,
            initial_focus: None,
        }
    }

    pub fn focus(mut self, focus: FocusHandle) -> Self {
        self.initial_focus = Some(focus);
        self
    }

    pub(super) fn can_close(&self) -> bool {
        !self.handle.is_busy()
    }
    pub(super) fn mark_closed(&self) {
        self.handle.state.open.set(false);
    }

    pub(super) fn render(&self, focus: &FocusHandle, cx: &mut Context<Client>) -> Modal {
        Modal::new(
            "content-modal",
            self.title.clone(),
            focus,
            self.can_close(),
            cx.listener(|client, _, window, cx| {
                client.dismiss_modal(window, cx);
                cx.notify();
            }),
        )
        .content_size(self.maximum, self.fit_content)
        .child(
            div()
                .id("modal-content-scroll")
                .w_full()
                .min_h_0()
                .min_w_0()
                .p(px(16.))
                .when(!self.fit_content, |body| body.flex_1().overflow_hidden())
                .when(self.fit_content, |body| body.overflow_y_scroll())
                .child(self.content.clone()),
        )
    }
}

impl Client {
    pub(in crate::app) fn present_content_modal(
        modal: ContentModal,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |client, window, cx| {
            if client.has_open_modal() {
                return;
            }
            modal.handle.state.open.set(true);
            client.modal_focus.focus(window, cx);
            if let Some(focus) = &modal.initial_focus {
                focus.focus(window, cx);
            }
            client.content_modal = Some(modal);
            cx.notify();
        });
    }

    pub(in crate::app) fn close_content_modal(
        &mut self,
        handle: &ModalHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .content_modal
            .as_ref()
            .is_some_and(|modal| Rc::ptr_eq(&modal.handle.state, &handle.state))
        {
            self.dismiss_modal(window, cx);
            cx.notify();
        }
    }
}
