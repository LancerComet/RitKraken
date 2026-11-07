use gpui_kit::{
    base::ResizableState,
    component::resizable::{h_resizable, resizable_panel},
    prelude::*,
    *,
};

use super::{HEADER, LINE, MUTED};
use crate::i18n::tr;

pub(in crate::app) const HEADER_HEIGHT: Pixels = px(30.);

#[derive(Clone, Copy)]
pub(in crate::app) struct Column {
    pub label: &'static str,
    /// A sized column keeps its width when the container changes. `None` fills the remainder.
    pub width: Option<Pixels>,
    pub minimum: Pixels,
}

pub(in crate::app) struct ResizableColumns {
    state: Entity<ResizableState>,
    _changed: Subscription,
}

impl ResizableColumns {
    pub fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        let state = cx.new(|_| ResizableState::default());
        let changed = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            _changed: changed,
        }
    }

    pub fn header(
        &self,
        id: impl Into<ElementId>,
        columns: &[Column],
        padding: Pixels,
    ) -> impl IntoElement {
        Header {
            id: id.into(),
            state: self.state.clone(),
            columns: columns.to_vec(),
            padding,
        }
    }

    pub fn widths<const N: usize>(&self, columns: &[Column; N], cx: &App) -> ColumnWidths<N> {
        let sizes = self.state.read(cx).sizes();
        ColumnWidths(std::array::from_fn(|index| {
            sizes
                .get(index)
                .copied()
                .or(columns[index].width)
                .map(|width| width.max(columns[index].minimum))
        }))
    }

    pub fn minimum_width<const N: usize>(&self, columns: &[Column; N], cx: &App) -> Pixels {
        let widths = self.widths(columns, cx);
        columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                if column.width.is_some() {
                    widths.get(index).unwrap_or(column.minimum)
                } else {
                    column.minimum
                }
            })
            .sum()
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) struct ColumnWidths<const N: usize>([Option<Pixels>; N]);

impl<const N: usize> ColumnWidths<N> {
    pub fn get(&self, index: usize) -> Option<Pixels> {
        self.0[index]
    }

    pub fn cell(&self, index: usize, cell: Div) -> Div {
        cell.min_w_0().map(|cell| match self.get(index) {
            Some(width) => cell.w(width).flex_none(),
            None => cell.flex_1(),
        })
    }
}

#[derive(IntoElement)]
struct Header {
    id: ElementId,
    state: Entity<ResizableState>,
    columns: Vec<Column>,
    padding: Pixels,
}

impl RenderOnce for Header {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let minimums: Vec<_> = self
            .columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| {
                self.state
                    .read(cx)
                    .sizes()
                    .get(index)
                    .filter(|width| **width < column.minimum)
                    .map(|_| (index, column.minimum))
            })
            .collect();
        if !minimums.is_empty() {
            let state = self.state.clone();
            // Newly loaded graph lanes can raise a column's minimum. Let layout
            // install the new ranges first, then redistribute through the same
            // resize engine used by a pointer drag.
            window.defer(cx, move |window, cx| {
                state.update(cx, |state, cx| {
                    for (index, minimum) in minimums {
                        if state
                            .sizes()
                            .get(index)
                            .is_some_and(|width| *width < minimum)
                        {
                            state.resize_panel(index, minimum, window, cx);
                        }
                    }
                });
            });
        }
        div()
            .w_full()
            .h(HEADER_HEIGHT)
            .flex_shrink_0()
            .bg(rgb(HEADER))
            .text_size(px(12.))
            .text_color(rgb(MUTED))
            .font_weight(FontWeight::NORMAL)
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                h_resizable(self.id)
                    .with_state(&self.state)
                    .children(self.columns.iter().map(|column| {
                        resizable_panel()
                            .size_range(column.minimum..Pixels::MAX)
                            .when_some(column.width, |panel, width| panel.size(width).fixed())
                            .child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .px(self.padding)
                                    .flex()
                                    .items_center()
                                    .child(div().w_full().truncate().child(tr!(column.label))),
                            )
                    })),
            )
    }
}
