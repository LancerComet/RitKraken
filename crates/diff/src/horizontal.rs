use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, Axis, Bounds, Context, Div, Entity, InputEvent as _, InteractiveElement as _,
    IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div, point,
    prelude::FluentBuilder as _, px,
};
use gpui_base::AutoScroll;
use gpui_component::scroll::{ScrollableMask, Scrollbar, ScrollbarMode};

use crate::{DiffSide, DiffState, split::SplitLayout};

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

/// Horizontal counterpart of [`AutoScroll::compute_delta`]: positive scrolls
/// toward the end of the line, negative toward its start, and a center dead
/// zone returns `None`.
pub fn horizontal_auto_scroll_delta(x: Pixels, bounds: Bounds<Pixels>) -> Option<Pixels> {
    const MIN_SPEED: f32 = 12.0;
    const MAX_SPEED: f32 = 64.0;
    const INNER_ZONE: f32 = 16.0;
    const OUTER_RAMP: f32 = 80.0;

    if bounds.size.width <= px(0.) {
        return None;
    }
    let inner = px(INNER_ZONE).min(bounds.size.width / 4.);
    let right_trigger = bounds.right() - inner;
    let left_trigger = bounds.left() + inner;

    if x > right_trigger {
        let t = ((x - right_trigger) / (inner + px(OUTER_RAMP))).min(1.0);
        Some(px(MIN_SPEED + t * (MAX_SPEED - MIN_SPEED)))
    } else if x < left_trigger {
        let t = ((left_trigger - x) / (inner + px(OUTER_RAMP))).min(1.0);
        Some(px(-(MIN_SPEED + t * (MAX_SPEED - MIN_SPEED))))
    } else {
        None
    }
}

/// Applies one auto-scroll tick to a horizontal scroll offset, clamped to the
/// handle's `[-max_x, 0]` range. `None` means the edge is reached.
pub fn step_horizontal_offset(
    current_x: Pixels,
    max_x: Pixels,
    delta: Pixels,
) -> Option<Pixels> {
    let max_x = max_x.max(px(0.));
    let current = current_x.clamp(-max_x, px(0.));
    let next = (current - delta).clamp(-max_x, px(0.));
    (next != current).then_some(next)
}

#[derive(Default)]
pub(crate) struct HorizontalAutoScroll {
    auto_scroll: AutoScroll,
    refresh_selection: bool,
}

impl HorizontalAutoScroll {
    pub(crate) fn is_active(&self) -> bool {
        self.auto_scroll.is_active() || self.refresh_selection
    }
}

impl DiffState {
    pub(crate) fn stop_horizontal_auto_scroll(&mut self) {
        self.horizontal_auto_scroll.auto_scroll.stop();
        self.horizontal_auto_scroll.refresh_selection = false;
    }

    fn selection_drag_side(&self, cx: &App) -> Option<DiffSide> {
        if self.selection().has_local_selection(cx) {
            return None;
        }
        let snapshot = self.selection().snapshot(cx)?;
        if !snapshot.is_selecting() || snapshot.anchor().entity_id() != Some(self.selection().entity_id()) {
            return None;
        }
        Some(crate::diff::decode_side(
            snapshot.anchor().content_key()?.value(),
        ))
    }

    fn horizontal_handle_for(&self, split: bool, side: DiffSide) -> ScrollHandle {
        if split {
            self.split_scroll().handle(side).clone()
        } else {
            self.horizontal_scroll().clone()
        }
    }

    pub(crate) fn update_horizontal_auto_scroll(
        &mut self,
        position: Point<Pixels>,
        soft_wrap: bool,
        split: bool,
        cx: &mut Context<Self>,
    ) {
        if soft_wrap || cx.has_active_drag() {
            self.stop_horizontal_auto_scroll();
            return;
        }
        let Some(side) = self.selection_drag_side(cx) else {
            self.stop_horizontal_auto_scroll();
            return;
        };
        let handle = self.horizontal_handle_for(split, side);
        let max_x = handle.max_offset().x.max(px(0.));
        if max_x <= px(0.) {
            self.stop_horizontal_auto_scroll();
            return;
        }
        let viewport = {
            let geometry = self.geometry().borrow();
            geometry
                .code_viewport(side)
                .or_else(|| {
                    (!split).then(|| {
                        geometry.code_viewport(match side {
                            DiffSide::Original => DiffSide::Modified,
                            DiffSide::Modified => DiffSide::Original,
                        })
                    })?
                })
                .unwrap_or_else(|| handle.bounds())
        };
        let delta = horizontal_auto_scroll_delta(position.x, viewport);
        self.horizontal_auto_scroll
            .auto_scroll
            .set(delta, cx, move |delta, state, cx| {
                state.step_horizontal_auto_scroll(delta, split, cx);
            });
    }

    fn step_horizontal_auto_scroll(
        &mut self,
        delta: Pixels,
        split: bool,
        cx: &mut Context<Self>,
    ) {
        if cx.has_active_drag() {
            self.stop_horizontal_auto_scroll();
            return;
        }
        let Some(side) = self.selection_drag_side(cx) else {
            self.stop_horizontal_auto_scroll();
            return;
        };
        let handle = self.horizontal_handle_for(split, side);
        let offset = handle.offset();
        let Some(next_x) = step_horizontal_offset(offset.x, handle.max_offset().x, delta) else {
            return;
        };
        handle.set_offset(point(next_x, offset.y));
        self.horizontal_auto_scroll.refresh_selection = true;
        cx.notify();
    }

    /// The scroll moved painted text under a stationary pointer, so replay
    /// the drag position once the new geometry is in place.
    pub(crate) fn finish_horizontal_auto_scroll_refresh(
        &mut self,
        soft_wrap: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if soft_wrap {
            self.stop_horizontal_auto_scroll();
            return;
        }
        if !self.horizontal_auto_scroll.refresh_selection {
            return;
        }
        self.horizontal_auto_scroll.refresh_selection = false;
        if self.selection_drag_side(cx).is_none() {
            self.stop_horizontal_auto_scroll();
            return;
        }
        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, cx| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let should_dispatch = state.update(cx, |state, cx| {
                if state.selection_drag_side(cx).is_none() || cx.has_active_drag() {
                    state.stop_horizontal_auto_scroll();
                    false
                } else {
                    true
                }
            });
            if should_dispatch {
                let input = MouseMoveEvent {
                    position: window.mouse_position(),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: window.modifiers(),
                }
                .to_platform_input();
                window.dispatch_event(input, cx);
            }
        });
    }
}

/// Registers window-level mouse listeners every frame so a drag leaving the
/// diff still scrolls, and releasing anywhere stops it.
pub(crate) fn drag_scroll_listener(
    state: Entity<DiffState>,
    soft_wrap: bool,
    split: bool,
) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _cx| {
            window.on_mouse_event({
                let state = state.downgrade();
                move |event: &MouseMoveEvent, phase, _, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    let pressed = event.pressed_button == Some(MouseButton::Left);
                    let position = event.position;
                    if pressed {
                        let should_update = {
                            let view = state.read(cx);
                            view.horizontal_auto_scroll.is_active()
                                || (!soft_wrap
                                    && !cx.has_active_drag()
                                    && view.selection_drag_side(cx).is_some())
                        };
                        if should_update {
                            let _ = state.update(cx, |state, cx| {
                                state.update_horizontal_auto_scroll(position, soft_wrap, split, cx);
                            });
                        }
                    } else if state.read(cx).horizontal_auto_scroll.is_active() {
                        let _ = state.update(cx, |state, _| state.stop_horizontal_auto_scroll());
                    }
                }
            });
            window.on_mouse_event({
                let state = state.downgrade();
                move |_: &MouseUpEvent, phase, _, cx| {
                    if !phase.capture() {
                        return;
                    }
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    if state.read(cx).horizontal_auto_scroll.is_active() {
                        let _ = state.update(cx, |state, _| state.stop_horizontal_auto_scroll());
                    }
                }
            });
        },
    )
    .absolute()
    .size(px(0.))
    .into_any_element()
}
