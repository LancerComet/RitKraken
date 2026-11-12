use std::sync::{Arc, OnceLock};

use gpui_kit::{prelude::*, *};

use super::PAPER;
use crate::i18n::tr;

// The executable icon and loading page share the original colour SVG.
static LOGO: OnceLock<Arc<RenderImage>> = OnceLock::new();

pub(in crate::app) fn opening_message() -> Div {
    div()
        .text_size(px(14.))
        .line_height(px(22.))
        .child(tr!("app.status.opening"))
}

pub(in crate::app) fn loading_page(content: impl IntoElement, cx: &App) -> Div {
    // Decode this small bundled asset once, before painting. An asynchronous
    // image loader can otherwise miss the entire repository-loading screen.
    let logo = LOGO.get_or_init(|| {
        cx.svg_renderer()
            .render_single_frame(include_bytes!("../../../assets/ritkraken.svg"), 0.25)
            .expect("The embedded application logo must be valid SVG")
    });
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(PAPER))
        .child(
            div()
                .w(px(560.))
                .max_w_full()
                .px(px(24.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(24.))
                .text_center()
                .child(img(logo.clone()).size(px(120.)).flex_shrink_0())
                .child(content),
        )
}
