mod content;

pub(super) use content::{ContentModal, ModalHandle};

use std::time::Duration;

use gpui_kit::{
    base::{
        animation::cubic_bezier,
        motion::{Presence, Transition},
    },
    *,
};

use super::{
    Client, PreferenceDraft, repository_manager::ManagerMode, widgets::modal::modal_layer,
};

const MODAL_DURATION: Duration = Duration::from_millis(200);

pub(super) enum ClosingModal {
    Preferences(Box<PreferenceDraft>),
    Repositories(ManagerMode),
    Content(ContentModal),
}

impl Client {
    pub(super) fn has_open_modal(&self) -> bool {
        self.preferences.is_some() || self.manager.mode.is_some() || self.content_modal.is_some()
    }

    pub(super) fn modal_is_present(&self) -> bool {
        self.has_open_modal() || self.closing_modal.is_some()
    }

    pub(super) fn dismiss_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .content_modal
            .as_ref()
            .is_some_and(|modal| !modal.can_close())
        {
            return;
        }
        let content = self.content_modal.take();
        if let Some(modal) = &content {
            modal.mark_closed();
        }
        let preferences = self.preferences.take();
        let repositories = self.manager.mode.take();
        if let Some(modal) = preferences
            .map(|draft| ClosingModal::Preferences(Box::new(draft)))
            .or(repositories.map(ClosingModal::Repositories))
            .or(content.map(ClosingModal::Content))
        {
            self.closing_modal = Some(modal);
        }
        self.focus.focus(window, cx);
    }

    pub(super) fn render_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !self.modal_is_present() {
            self.modal_compositor.clear(window);
            return None;
        }
        let open = self.has_open_modal();
        let motion = Presence::new("ritkraken-modal", open)
            .transition(Transition::new(MODAL_DURATION).ease(cubic_bezier(0.22, 0.58, 0.12, 0.98)))
            .sample(window, cx);
        if open || !motion.should_render() || motion.progress == 0.0 {
            self.closing_modal = None;
        }
        if !self.modal_is_present() {
            self.modal_compositor.clear(window);
            return None;
        }

        let composition = if open && motion.progress == 1.0 {
            self.modal_compositor.finish_entering(window);
            None
        } else {
            self.modal_compositor.update(window, motion.progress)
        };
        if !open && composition.is_none() {
            self.closing_modal = None;
            return None;
        }

        let (preferences, repositories, content) = match &self.closing_modal {
            Some(ClosingModal::Preferences(draft)) => (Some(draft.as_ref()), None, None),
            Some(ClosingModal::Repositories(mode)) => (None, Some(*mode), None),
            Some(ClosingModal::Content(content)) => (None, None, Some(content)),
            None => (
                self.preferences.as_ref(),
                self.manager.mode,
                self.content_modal.as_ref(),
            ),
        };
        let surface = if let Some(draft) = preferences {
            self.preferences_view(draft, cx).into_any_element()
        } else if let Some(mode) = repositories {
            self.manager_view(mode, window, cx).into_any_element()
        } else if let Some(content) = content {
            content.render(&self.modal_focus, cx).into_any_element()
        } else {
            self.modal_compositor.clear(window);
            return None;
        };
        Some(modal_layer(surface, composition, motion.progress, !open))
    }
}
