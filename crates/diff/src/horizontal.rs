use std::{cell::Cell, rc::Rc};

use gpui::{
    Axis, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, div, point,
    prelude::FluentBuilder as _, px,
};
use gpui_component::scroll::{ScrollableMask, Scrollbar, ScrollbarMode};

use crate::{DiffSide, split::SplitLayout};

const SIDES: [DiffSide; 2] = [DiffSide::Original, DiffSide::Modified];

pub(crate) fn scrollbar(handle: &ScrollHandle) -> Scrollbar {
    Scrollbar::horizontal(handle).mode(ScrollbarMode::Always)
}

#[derive(Clone)]
pub(crate) struct SplitScroll {
    handles: [ScrollHandle; 2],
    widths: [Pixels; 2],
    active: Rc<Cell<DiffSide>>,
}

impl Default for SplitScroll {
    fn default() -> Self {
        Self {
            handles: std::array::from_fn(|_| ScrollHandle::new()),
            widths: [px(0.); 2],
            active: Rc::new(Cell::new(DiffSide::Modified)),
        }
    }
}

impl SplitScroll {
    pub(crate) fn handle(&self, side: DiffSide) -> &ScrollHandle {
        &self.handles[index(side)]
    }

    pub(crate) fn active_handle(&self) -> &ScrollHandle {
        self.handle(self.active.get())
    }

    pub(crate) fn set_widths(&mut self, widths: [Pixels; 2]) {
        self.widths = widths;
    }

    pub(crate) fn grow_widths(&mut self, widths: [Pixels; 2]) -> bool {
        let next = std::array::from_fn(|ix| self.widths[ix].max(widths[ix]));
        let changed = next != self.widths;
        self.widths = next;
        changed
    }

    pub(crate) fn offsets(&self) -> [Point<Pixels>; 2] {
        std::array::from_fn(|ix| self.handles[ix].offset())
    }

    pub(crate) fn restore(&self, offsets: [Point<Pixels>; 2]) {
        for (handle, offset) in self.handles.iter().zip(offsets) {
            handle.set_offset(offset);
        }
    }

    pub(crate) fn reset(&self) {
        self.restore([point(px(0.), px(0.)); 2]);
    }

    /// These invisible extents must be prepainted before the code reads the
    /// clamped offsets and before the scrollbar reads each pane's bounds.
    pub(crate) fn trackers(&self, split: SplitLayout) -> Div {
        panes(split, |side| {
            div()
                .id(("diff-pane-extent", index(side)))
                .size_full()
                .overflow_x_hidden()
                .track_scroll(self.handle(side))
                .child(div().w(self.widths[index(side)]))
        })
    }

    pub(crate) fn controls(&self, split: SplitLayout, scrollable: bool) -> Div {
        panes(split, |side| {
            let active = self.active.clone();
            div()
                .id(("diff-pane-controls", index(side)))
                .size_full()
                .on_mouse_move(move |_, _, _| active.set(side))
                .when(scrollable, |pane| {
                    pane.child(
                        ScrollableMask::new(Axis::Horizontal, self.handle(side))
                            .id(("diff-pane-wheel", index(side))),
                    )
                    .child(scrollbar(self.handle(side)))
                })
        })
    }
}

fn panes<E: IntoElement>(split: SplitLayout, mut render: impl FnMut(DiffSide) -> E) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .children(SIDES.map(|side| {
            div()
                .relative()
                .w(split.width(side))
                .h_full()
                .min_w_0()
                .flex_shrink_0()
                .child(render(side))
        }))
}

fn index(side: DiffSide) -> usize {
    match side {
        DiffSide::Original => 0,
        DiffSide::Modified => 1,
    }
}
