use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

use gpui_kit::{base::ElementExt, prelude::*, *};
use ritkraken_git::{self as git, FileTarget, Repo};

use super::{FileReview, ImageDiffMode, view::pill_group};
use crate::app::{
    Client, Load, Message, RepoTab,
    image::decode_image,
    theme::{Palette, palette},
    widgets::{Tone, button, error_text, note},
};
use crate::i18n::tr;

const MAX_IMAGE_SIDE: u32 = 16384;
const MAX_IMAGE_ALLOC: u64 = 256 * 1024 * 1024;
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

pub(super) struct ImageSide {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
    bytes: usize,
}

pub(in crate::app) struct ImageDiff {
    old: Option<Result<ImageSide, Message>>,
    new: Option<Result<ImageSide, Message>>,
}

pub(super) fn is_image_file(file: &FileTarget) -> bool {
    let change = &file.change;
    change.code != b'U'
        && (has_image_extension(&change.path)
            || change.previous.as_deref().is_some_and(has_image_extension))
}

fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

pub(super) fn load_image_diff(repo: &Repo, target: &FileTarget) -> git::Result<Arc<ImageDiff>> {
    let versions = repo.file_versions(target)?;
    let side = |bytes: &Vec<u8>| {
        decode_image(bytes, MAX_IMAGE_SIDE, MAX_IMAGE_ALLOC)
            .map(|decoded| ImageSide {
                image: decoded.image,
                width: decoded.width,
                height: decoded.height,
                bytes: bytes.len(),
            })
            .ok_or(Message::Key("app.review.image_decode_failed"))
    };
    Ok(Arc::new(ImageDiff {
        old: versions.old.as_ref().map(side),
        new: versions.new.as_ref().map(side),
    }))
}

fn swipe_available(review: &FileReview) -> bool {
    matches!(
        &review.image,
        Some(Load::Ready(diff))
            if matches!(&diff.old, Some(Ok(_))) && matches!(&diff.new, Some(Ok(_)))
    )
}

pub(super) fn mode_selector(
    tab: &RepoTab,
    review: &FileReview,
    p: Palette,
    cx: &Context<Client>,
) -> Div {
    let id = tab.id;
    let swipe = swipe_available(review);
    pill_group(p).children(ImageDiffMode::ALL.into_iter().map(|mode| {
        button(
            mode.id(),
            tr!(mode.label_key()),
            Tone::Plain.selected_when(tab.review.image_mode == mode),
            mode != ImageDiffMode::Swipe || swipe,
            cx,
            cx.listener(move |this, _, _, cx| {
                if let Some(tab) = this.tab_mut(id) {
                    tab.review.image_mode = mode;
                    cx.notify();
                }
            }),
        )
        .rounded_full()
        .h(px(22.))
        .px(px(7.))
        .text_size(px(11.))
    }))
}

pub(super) fn content(tab: &RepoTab, review: &FileReview, cx: &mut Context<Client>) -> AnyElement {
    let p = palette(cx);
    match &review.image {
        Some(Load::Ready(diff)) => {
            if review.show_source {
                single(diff, p, cx)
            } else if swipe_available(review) && tab.review.image_mode == ImageDiffMode::Swipe {
                swipe_view(tab.id, review, diff, p, cx)
            } else {
                side_by_side(diff, p, cx)
            }
        }
        Some(Load::Failed(error)) => error_text(error, cx).into_any_element(),
        _ => note(tr!("app.review.loading_diff"), cx).into_any_element(),
    }
}

fn single(diff: &ImageDiff, p: Palette, cx: &App) -> AnyElement {
    match diff.new.as_ref().or(diff.old.as_ref()) {
        Some(Ok(side)) => div()
            .size_full()
            .flex()
            .flex_col()
            .child(fitted(&side.image))
            .child(caption(side, p))
            .into_any_element(),
        Some(Err(message)) => error_text(message, cx).into_any_element(),
        None => note(tr!("app.review.loading_diff"), cx).into_any_element(),
    }
}

fn side_by_side(diff: &ImageDiff, p: Palette, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .gap(px(8.))
        .p(px(8.))
        .children(
            diff.old
                .iter()
                .map(|side| panel("app.review.image_old", side, p, cx)),
        )
        .children(
            diff.new
                .iter()
                .map(|side| panel("app.review.image_new", side, p, cx)),
        )
        .into_any_element()
}

fn panel(label_key: &'static str, side: &Result<ImageSide, Message>, p: Palette, cx: &App) -> Div {
    let body = match side {
        Ok(side) => fitted(&side.image),
        Err(message) => div()
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .child(error_text(message, cx))
            .into_any_element(),
    };
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .rounded(px(10.))
        .border_1()
        .border_color(rgb(p.border))
        .bg(rgb(p.surface))
        .overflow_hidden()
        .child(
            div()
                .px(px(10.))
                .py(px(5.))
                .border_b_1()
                .border_color(rgb(p.border))
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(p.text_muted))
                .child(tr!(label_key)),
        )
        .child(body)
        .when_some(side.as_ref().ok(), |panel, side| {
            panel.child(caption(side, p))
        })
}

fn fitted(image: &Arc<RenderImage>) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(
            img(image.clone())
                .size_full()
                .object_fit(ObjectFit::ScaleDown),
        )
        .into_any_element()
}

fn caption(side: &ImageSide, p: Palette) -> Div {
    div()
        .px(px(10.))
        .py(px(5.))
        .border_t_1()
        .border_color(rgb(p.border))
        .text_size(px(11.))
        .text_color(rgb(p.text_muted))
        .child(tr!(
            "app.review.image_dimensions",
            width = side.width,
            height = side.height,
            size = format_bytes(side.bytes)
        ))
}

fn swipe_view(
    id: u64,
    review: &FileReview,
    diff: &ImageDiff,
    p: Palette,
    cx: &mut Context<Client>,
) -> AnyElement {
    let (Some(Ok(old)), Some(Ok(new))) = (&diff.old, &diff.new) else {
        return side_by_side(diff, p, cx);
    };
    let split = review.swipe_split.clamp(0., 1.);
    let bounds = Rc::new(Cell::new(Bounds::default()));
    div()
        .id(("image-swipe", id))
        .size_full()
        .overflow_hidden()
        .cursor_ew_resize()
        .on_prepaint({
            let bounds = bounds.clone();
            move |box_bounds, _, _| bounds.set(box_bounds)
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener({
                let bounds = bounds.clone();
                move |this, event: &MouseDownEvent, _, cx| {
                    update_swipe_split(this, id, &bounds, event.position, cx)
                }
            }),
        )
        .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
            if event.dragging() {
                update_swipe_split(this, id, &bounds, event.position, cx);
            }
        }))
        .child(
            img(old.image.clone())
                .absolute()
                .inset_0()
                .size_full()
                .object_fit(ObjectFit::ScaleDown),
        )
        .when(split > 0.001, |view| {
            view.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(split))
                    .overflow_hidden()
                    .child(
                        div().h_full().w(relative(1. / split.max(0.001))).child(
                            img(new.image.clone())
                                .size_full()
                                .object_fit(ObjectFit::ScaleDown),
                        ),
                    ),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(split))
                .w(px(2.))
                .ml(px(-1.))
                .bg(rgb(p.accent)),
        )
        .child(
            div()
                .absolute()
                .left(relative(split))
                .top(relative(0.5))
                .ml(px(-9.))
                .mt(px(-9.))
                .size(px(18.))
                .rounded_full()
                .border_1()
                .border_color(rgb(p.accent))
                .bg(rgb(p.canvas))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .child(div().w(px(1.5)).h(px(7.)).rounded_full().bg(rgb(p.accent)))
                .child(div().w(px(1.5)).h(px(7.)).rounded_full().bg(rgb(p.accent))),
        )
        .into_any_element()
}

fn update_swipe_split(
    this: &mut Client,
    id: u64,
    bounds: &Cell<Bounds<Pixels>>,
    position: Point<Pixels>,
    cx: &mut Context<Client>,
) {
    let bounds = bounds.get();
    if bounds.size.width <= px(0.) {
        return;
    }
    let split = (position.x - bounds.origin.x).clamp(px(0.), bounds.size.width) / bounds.size.width;
    if let Some(review) = this.tab_mut(id).and_then(|tab| tab.review.active.as_mut()) {
        review.swipe_split = split;
        cx.notify();
    }
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / KIB)
    } else {
        format!("{:.1} MB", bytes as f64 / (KIB * KIB))
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn formats_byte_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(2_500_000), "2.4 MB");
    }
}
