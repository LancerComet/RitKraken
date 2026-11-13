use gpui::{Context, Window, point, px};

use crate::DiffState;

impl DiffState {
    /// Reuse the selection surface's measured text positions. This also finds
    /// matches inside a wrapped line taller than the viewport, without a
    /// second text-shaping implementation or a guessed character width.
    pub(crate) fn reveal_search_target(
        &mut self,
        wrap: bool,
        split: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search.reveal || !self.search.open {
            return;
        }
        let Some(hit) = self
            .search
            .model
            .as_ref()
            .and_then(super::DiffSearch::current)
        else {
            return;
        };
        let side = hit.position.side();
        let target = self.geometry().borrow().search_target(
            hit.file,
            side,
            hit.line_index,
            hit.range.clone(),
        );
        let Some((viewport, start, end, line_height)) = target else {
            return;
        };
        let margin = px(12.);
        let vertical = if wrap {
            if start.y + line_height > viewport.bottom() - margin {
                start.y + line_height - viewport.bottom() + margin
            } else if start.y < viewport.top() + margin {
                start.y - viewport.top() - margin
            } else {
                px(0.)
            }
        } else {
            px(0.)
        };
        let horizontal = if !wrap {
            let handle = if split {
                self.split_scroll().handle(side)
            } else {
                self.horizontal_scroll()
            };
            let pane = handle.bounds();
            if pane.size.width <= px(0.) {
                return;
            }
            let delta = if end.x > pane.right() - margin {
                (end.x - pane.right() + margin).min(start.x - pane.left() - margin)
            } else if start.x < pane.left() + margin {
                start.x - pane.left() - margin
            } else {
                px(0.)
            };
            let offset = handle.offset();
            Some((
                handle.clone(),
                point((offset.x - delta).min(px(0.)), offset.y),
            ))
        } else {
            None
        };
        self.search.reveal = false;
        let revision = self.search.revision;
        // Layout finalizes its own scroll offsets after prepaint. Apply the
        // measured reveal afterwards so layout cannot overwrite it. A newer
        // query or navigation invalidates this pending reveal.
        cx.defer_in(window, move |state, _, cx| {
            if !state.search.open || state.search.revision != revision {
                return;
            }
            if let Some((handle, offset)) = horizontal {
                handle.set_offset(offset);
            } else {
                state.list().scroll_by(vertical);
            }
            cx.notify();
        });
    }
}
