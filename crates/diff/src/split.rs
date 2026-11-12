use gpui::{
    AppContext as _, Axis, Bounds, Context, DefiniteLength, Div, DragMoveEvent, Empty, EntityId,
    InteractiveElement as _, ParentElement as _, Pixels, Render, Styled as _, Window, div, px,
    relative,
};
use gpui_component::resizable::resize_handle;

use crate::{DiffSide, DiffState};

const MIN_PANE_WIDTH: Pixels = px(96.);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SplitLayout {
    original: f32,
}

impl Default for SplitLayout {
    fn default() -> Self {
        Self { original: 0.5 }
    }
}

impl SplitLayout {
    pub(crate) fn from_pointer(x: Pixels, viewport: Bounds<Pixels>) -> Option<Self> {
        let width = viewport.size.width;
        (width > px(0.)).then(|| {
            let original = f32::from(x - viewport.left()) / f32::from(width);
            Self { original }.constrained(width)
        })
    }

    pub(crate) fn constrained(self, width: Pixels) -> Self {
        let min = (f32::from(MIN_PANE_WIDTH) / f32::from(width)).min(0.5);
        Self {
            original: self.original.clamp(min, 1. - min),
        }
    }

    pub(crate) fn width(self, side: DiffSide) -> DefiniteLength {
        relative(match side {
            DiffSide::Original => self.original,
            DiffSide::Modified => 1. - self.original,
        })
    }

    pub(crate) fn divider(self, cx: &Context<DiffState>) -> Div {
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(relative(self.original))
            .w(px(1.))
            .child(
                resize_handle("diff-split-divider", Axis::Horizontal)
                    .on_drag(DividerDrag(cx.entity_id()), |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| DividerPreview)
                    })
                    .on_drag_move(cx.listener(
                        |state, event: &DragMoveEvent<DividerDrag>, _, cx| {
                            if event.drag(cx).0 == cx.entity_id() {
                                state.resize_split(event.event.position.x, cx);
                            }
                        },
                    )),
            )
    }
}

struct DividerDrag(EntityId);

struct DividerPreview;

impl Render for DividerPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        Empty
    }
}
