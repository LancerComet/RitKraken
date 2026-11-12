use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    App, Bounds, Context, Div, InteractiveElement as _, ListOffset, ListState, MouseButton,
    ParentElement as _, Pixels, Styled as _, Window, canvas, div, fill, point,
    prelude::FluentBuilder as _, px, size,
};
use gpui_component::{
    ActiveTheme as _,
    scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode},
};

use super::{Overview, Segment};
use crate::DiffState;

const TRACK_WIDTH: Pixels = px(16.);
const MARK_WIDTH: Pixels = px(8.);

#[derive(Clone, Copy)]
struct Band {
    top: usize,
    end: usize,
    flags: u8,
}

#[derive(Clone, Default)]
pub(super) struct PaintCache(Rc<RefCell<Option<([u32; 3], Rc<[Band]>)>>>);

struct Projection {
    content: Pixels,
    viewport_top: Pixels,
    offset: Pixels,
    rows: usize,
    height: usize,
}

impl Projection {
    fn new(scroll: &ListState, height: Pixels, rows: usize) -> Self {
        Self {
            content: scroll.content_size().height,
            viewport_top: scroll.viewport_bounds().top(),
            offset: scroll.offset().y,
            rows: rows.max(1),
            height: f32::from(height).ceil().max(1.) as usize,
        }
    }

    fn range(&self, segment: &Segment, scroll: &ListState) -> std::ops::Range<usize> {
        let (top, bottom) = match (
            scroll.bounds_for_item(segment.rows.start),
            scroll.bounds_for_item(segment.rows.end - 1),
        ) {
            (Some(first), Some(last)) if self.content > px(0.) => (
                f32::from(first.top() - self.viewport_top - self.offset) / f32::from(self.content),
                f32::from(last.bottom() - self.viewport_top - self.offset)
                    / f32::from(self.content),
            ),
            _ => (
                segment.rows.start as f32 / self.rows as f32,
                segment.rows.end as f32 / self.rows as f32,
            ),
        };
        let start = ((top.clamp(0., 1.) * self.height as f32).floor() as usize)
            .min(self.height.saturating_sub(2));
        let end = ((bottom.clamp(0., 1.) * self.height as f32).ceil() as usize)
            .max(start + 2)
            .min(self.height);
        start..end
    }
}

impl Overview {
    fn bands(&self, height: Pixels, scroll: &ListState) -> Rc<[Band]> {
        let content = scroll.content_size().height;
        let rows = f32::from(height).ceil().max(0.) as usize;
        if rows == 0 || content <= px(0.) {
            return Rc::default();
        }
        let key = [
            f32::from(height).to_bits(),
            f32::from(content).to_bits(),
            f32::from(scroll.viewport_bounds().size.width).to_bits(),
        ];
        if let Some((old, bands)) = &*self.cache.0.borrow()
            && *old == key
        {
            return bands.clone();
        }
        let mut deltas = vec![[0i32; 2]; rows + 1];
        let projection = Projection::new(scroll, height, self.row_count);
        for segment in self.segments.iter() {
            let range = projection.range(segment, scroll);
            for (side, flag) in [1, 2].into_iter().enumerate() {
                if segment.flags & flag != 0 {
                    deltas[range.start][side] += 1;
                    deltas[range.end][side] -= 1;
                }
            }
        }
        let mut active = [0; 2];
        let mut bands: Vec<Band> = Vec::new();
        for (row, delta) in deltas.into_iter().take(rows).enumerate() {
            for side in 0..2 {
                active[side] += delta[side];
            }
            let flags = u8::from(active[0] > 0) | (u8::from(active[1] > 0) << 1);
            if flags == 0 {
                continue;
            }
            if let Some(last) = bands
                .last_mut()
                .filter(|last| last.end == row && last.flags == flags)
            {
                last.end += 1;
            } else {
                bands.push(Band {
                    top: row,
                    end: row + 1,
                    flags,
                });
            }
        }
        let bands: Rc<[Band]> = bands.into();
        *self.cache.0.borrow_mut() = Some((key, bands.clone()));
        bands
    }

    fn row_at(&self, y: Pixels, height: Pixels, scroll: &ListState) -> Option<usize> {
        let projection = Projection::new(scroll, height, self.row_count);
        let y = f32::from(y);
        self.segments
            .iter()
            .map(|segment| {
                let range = projection.range(segment, scroll);
                let distance = (range.start as f32 - y).max(0.).max(y - range.end as f32);
                let fraction = ((y - range.start as f32) / range.len().max(1) as f32).clamp(0., 1.);
                let row = segment.rows.start
                    + ((fraction * segment.rows.len() as f32) as usize).min(segment.rows.len() - 1);
                (distance, row)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, row)| row)
    }
}

pub(crate) fn scrollbar(
    overview: Overview,
    scroll: &ListState,
    cx: &mut Context<DiffState>,
) -> gpui::Stateful<Div> {
    let has_marks = !overview.segments.is_empty();
    let scroll = scroll.clone();
    let bounds = Rc::new(Cell::new(Bounds::default()));
    let painted_bounds = bounds.clone();
    let painted_scroll = scroll.clone();
    let click_scroll = scroll.clone();
    let click_overview = overview.clone();
    div()
        .id("diff-overview-scrollbar")
        .relative()
        .h_full()
        .w(TRACK_WIDTH + if has_marks { MARK_WIDTH } else { px(0.) })
        .flex_shrink_0()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .when(has_marks, |rail| {
            rail.child(
                div()
                    .id("diff-change-map")
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(MARK_WIDTH)
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            let bounds = bounds.get();
                            if bounds.size.height <= px(0.) {
                                return;
                            }
                            if let Some(row) = click_overview.row_at(
                                event.position.y - bounds.top(),
                                bounds.size.height,
                                &click_scroll,
                            ) {
                                click_scroll.scroll_to(ListOffset {
                                    item_ix: row,
                                    offset_in_item: px(0.),
                                });
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                painted_bounds.set(bounds);
                                overview.bands(bounds.size.height, &painted_scroll)
                            },
                            |bounds, bands, window: &mut Window, cx: &mut App| {
                                for band in bands.iter() {
                                    let y = bounds.top() + px(band.top as f32);
                                    let height =
                                        px((band.end - band.top) as f32).min(bounds.bottom() - y);
                                    let width =
                                        bounds.size.width / if band.flags == 3 { 2. } else { 1. };
                                    for (flag, color) in
                                        [(1, cx.theme().danger), (2, cx.theme().success)]
                                    {
                                        if band.flags & flag == 0 {
                                            continue;
                                        }
                                        let x = bounds.left()
                                            + if band.flags == 3 && flag == 2 {
                                                width
                                            } else {
                                                px(0.)
                                            };
                                        window.paint_quad(fill(
                                            Bounds::new(point(x, y), size(width, height)),
                                            color,
                                        ));
                                    }
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
        })
        .child(
            div()
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .w(TRACK_WIDTH)
                .child(
                    Scrollbar::vertical(&scroll)
                        .viewport_from_layout()
                        .mode(ScrollbarMode::Always),
                ),
        )
}
