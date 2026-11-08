use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::*;

/// A ref stack opens in the hover event itself, and stays open while its menu is in use.
#[derive(Default)]
pub(super) struct RefHover {
    pub expanded: bool,
    pub trigger: Rc<Cell<Bounds<Pixels>>>,
    pub primary_width: Rc<Cell<Pixels>>,
    trigger_hovered: bool,
    content_hovered: bool,
    menu_open: bool,
    close: Option<Task<()>>,
}

impl RefHover {
    pub fn hover(
        &mut self,
        content: bool,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if content {
            self.content_hovered = hovered;
        } else {
            self.trigger_hovered = hovered;
        }
        self.close = None;
        if hovered {
            self.expanded = true;
            cx.notify();
        } else {
            self.close_when_idle(window, cx);
        }
    }

    pub fn menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = open;
        self.close = None;
        if !open {
            self.close_when_idle(window, cx);
        }
    }

    fn close_when_idle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.trigger_hovered || self.content_hovered || self.menu_open {
            return;
        }
        self.close = Some(cx.spawn_in(window, async move |this, cx| {
            // Allow crossing the small gap into the expanded list. Opening has no timer.
            cx.background_executor()
                .timer(Duration::from_millis(110))
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                if !this.trigger_hovered && !this.content_hovered && !this.menu_open {
                    this.expanded = false;
                    cx.notify();
                }
            });
        }));
    }
}
