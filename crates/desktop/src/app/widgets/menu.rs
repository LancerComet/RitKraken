//! Styling extensions for the pinned GPUI menu, using its existing interaction code.

use gpui_kit::{component::input::AnyInputState, *};

#[allow(dead_code)]
mod upstream {
    use super::MenuInputExt;

    include!(concat!(env!("OUT_DIR"), "/file-menu-modules.rs"));

    pub(crate) use context_menu::ContextMenuExt;
    pub(crate) use dropdown_menu::DropdownMenu;
    pub(crate) use popup_menu::{PopupMenu, PopupMenuItem};
}

pub(in crate::app) use upstream::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};

pub(in crate::app) fn menu_style(menu: PopupMenu, window: &Window) -> PopupMenu {
    menu.text_size(rems(0.875).to_pixels(window.rem_size()) - px(2.))
        .padding(rems(0.25).to_pixels(window.rem_size()) + px(2.))
}

// The toolkit's AnyInputState forwarding methods are private. This adapter
// forwards to the public input states without owning their selection.
trait MenuInputExt {
    fn menu_input_bounds(&self, cx: &App) -> Option<Bounds<Pixels>>;
    fn set_menu_selection_focus(&self, focus: Option<FocusHandle>, cx: &mut App);
}

impl MenuInputExt for AnyInputState {
    fn menu_input_bounds(&self, cx: &App) -> Option<Bounds<Pixels>> {
        match self {
            Self::Input(state) => Some(state.read(cx).input_bounds()),
            Self::Textarea(state) => Some(state.read(cx).input_bounds()),
            Self::Editor(state) => Some(state.read(cx).input_bounds()),
            Self::Otp(_) => None,
        }
    }

    fn set_menu_selection_focus(&self, focus: Option<FocusHandle>, cx: &mut App) {
        match self {
            Self::Input(state) => {
                state.update(cx, |state, cx| state.set_selection_focus(focus, cx))
            }
            Self::Textarea(state) => {
                state.update(cx, |state, cx| state.set_selection_focus(focus, cx))
            }
            Self::Editor(state) => {
                state.update(cx, |state, cx| state.set_selection_focus(focus, cx))
            }
            Self::Otp(_) => {}
        }
    }
}
