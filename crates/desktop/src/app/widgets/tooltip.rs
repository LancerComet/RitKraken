use gpui_kit::{component::tooltip::Tooltip, *};

pub(in crate::app) fn init(cx: &mut App) {
    cx.observe_new::<Tooltip>(|tooltip, _, _| {
        tooltip.style().text.font_size = Some(px(12.).into());
    })
    .detach();
}
