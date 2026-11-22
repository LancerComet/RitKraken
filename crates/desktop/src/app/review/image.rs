use std::{cell::Cell, path::Path, sync::Arc};

use gpui_kit::{base::ElementExt, prelude::*, *};
use ritkraken_git::{self as git, FileTarget, Repo};

use super::{FileReview, ImageDiffMode, view::pill_group};
use crate::app::{
    Client, Load, Message, RepoTab,
    image::{decode_image, decode_svg},
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreviewKind {
    Text,
    Raster,
    Svg,
}

pub(super) fn preview_kind(file: &FileTarget) -> PreviewKind {
    let change = &file.change;
    if change.code == b'U' {
        return PreviewKind::Text;
    }
    extension_kind(&change.path)
        .or_else(|| change.previous.as_deref().and_then(extension_kind))
        .unwrap_or(PreviewKind::Text)
}

fn extension_kind(path: &Path) -> Option<PreviewKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if extension == "svg" {
        Some(PreviewKind::Svg)
    } else {
        IMAGE_EXTENSIONS
            .contains(&extension.as_str())
            .then_some(PreviewKind::Raster)
    }
}

pub(super) fn load_image_diff(
    repo: &Repo,
    target: &FileTarget,
    kind: PreviewKind,
    renderer: SvgRenderer,
) -> git::Result<Arc<ImageDiff>> {
    let versions = repo.file_versions(target)?;
    let side = |bytes: &Vec<u8>| {
        let decoded = match kind {
            PreviewKind::Svg => decode_svg(bytes, &renderer),
            _ => decode_image(bytes, MAX_IMAGE_SIDE, MAX_IMAGE_ALLOC),
        };
        decoded
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
            if review.image_single {
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
        .flex()
        .overflow_hidden()
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

/// The bounds both swipe layers are drawn at inside the measured container:
/// one shared ScaleDown factor and shared centering so same-sized images
/// align pixel-for-pixel and different sizes still share scale and center.
fn swipe_layout(
    container: Size<Pixels>,
    old: (u32, u32),
    new: (u32, u32),
) -> (Bounds<Pixels>, Bounds<Pixels>) {
    let max_w = old.0.max(new.0).max(1) as f32;
    let max_h = old.1.max(new.1).max(1) as f32;
    let scale = (f32::from(container.width) / max_w)
        .min(f32::from(container.height) / max_h)
        .min(1.);
    let place = |width: u32, height: u32| {
        let width = width as f32 * scale;
        let height = height as f32 * scale;
        Bounds::new(
            point(
                px((f32::from(container.width) - width) / 2.),
                px((f32::from(container.height) - height) / 2.),
            ),
            size(px(width), px(height)),
        )
    };
    (place(old.0, old.1), place(new.0, new.1))
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
    let bounds = review.swipe_bounds.clone();
    let container = bounds.get().size;
    let mut view = div()
        .id(("image-swipe", id))
        .relative()
        .size_full()
        .overflow_hidden()
        .cursor_ew_resize()
        .on_prepaint({
            let bounds = bounds.clone();
            move |box_bounds, window, _| {
                if bounds.get().size != box_bounds.size {
                    bounds.set(box_bounds);
                    window.refresh();
                }
            }
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
        }));
    if container.width > px(0.) && container.height > px(0.) {
        let (old_bounds, new_bounds) =
            swipe_layout(container, (old.width, old.height), (new.width, new.height));
        let split_x = px(f32::from(container.width) * split);
        view = view
            .child(
                img(new.image.clone())
                    .absolute()
                    .left(new_bounds.origin.x)
                    .top(new_bounds.origin.y)
                    .w(new_bounds.size.width)
                    .h(new_bounds.size.height)
                    .object_fit(ObjectFit::Fill),
            )
            .when(split > 0.001, |view| {
                // The clip wrapper shares the container's origin, so the
                // old image keeps its unclipped coordinates inside it.
                view.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(split_x)
                        .overflow_hidden()
                        .child(
                            img(old.image.clone())
                                .absolute()
                                .left(old_bounds.origin.x)
                                .top(old_bounds.origin.y)
                                .w(old_bounds.size.width)
                                .h(old_bounds.size.height)
                                .object_fit(ObjectFit::Fill),
                        ),
                )
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(split_x)
                    .w(px(2.))
                    .ml(px(-1.))
                    .bg(rgb(p.accent)),
            )
            .child(
                div()
                    .absolute()
                    .left(split_x)
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
            );
    }
    view.into_any_element()
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
    use gpui_kit::{Bounds, Pixels, Point, point, px, size};
    use ritkraken_git::{Change, FileTarget, Scope};

    use super::{PreviewKind, format_bytes, preview_kind, swipe_layout};

    fn target(path: &str, previous: Option<&str>, code: u8) -> FileTarget {
        FileTarget {
            scope: Scope::Worktree,
            change: Change {
                path: path.into(),
                previous: previous.map(Into::into),
                code,
            },
        }
    }

    #[test]
    fn preview_kind_classifies_file_extensions() {
        assert_eq!(preview_kind(&target("a.svg", None, b'M')), PreviewKind::Svg);
        assert_eq!(preview_kind(&target("A.SVG", None, b'A')), PreviewKind::Svg);
        assert_eq!(preview_kind(&target("a.png", None, b'M')), PreviewKind::Raster);
        assert_eq!(preview_kind(&target("a.rs", None, b'M')), PreviewKind::Text);
        assert_eq!(preview_kind(&target("a.txt", None, b'M')), PreviewKind::Text);
    }

    #[test]
    fn preview_kind_checks_rename_source_and_conflicts() {
        assert_eq!(
            preview_kind(&target("renamed.txt", Some("a.svg"), b'R')),
            PreviewKind::Svg
        );
        assert_eq!(
            preview_kind(&target("a.svg", None, b'U')),
            PreviewKind::Text
        );
    }

    fn center(bounds: Bounds<Pixels>) -> Point<Pixels> {
        point(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        )
    }

    #[test]
    fn swipe_layout_centers_identical_images() {
        let (old, new) = swipe_layout(size(px(200.), px(100.)), (100, 50), (100, 50));
        assert_eq!(old, new);
        assert_eq!(old.origin, point(px(50.), px(25.)));
        assert_eq!(old.size, size(px(100.), px(50.)));
    }

    #[test]
    fn swipe_layout_scales_both_down_by_the_largest_dimensions() {
        let (old, new) = swipe_layout(size(px(100.), px(100.)), (200, 100), (50, 25));
        assert_eq!(old.size, size(px(100.), px(50.)));
        assert_eq!(new.size, size(px(25.), px(12.5)));
        assert_eq!(center(old), center(new));
    }

    #[test]
    fn swipe_layout_never_upscales() {
        let (old, new) = swipe_layout(size(px(200.), px(200.)), (10, 10), (20, 40));
        assert_eq!(old.size, size(px(10.), px(10.)));
        assert_eq!(new.size, size(px(20.), px(40.)));
        assert_eq!(center(old), point(px(100.), px(100.)));
        assert_eq!(center(new), center(old));
    }

    #[test]
    fn swipe_layout_shares_scale_and_center_for_different_sizes() {
        let (old, new) = swipe_layout(size(px(300.), px(200.)), (400, 200), (200, 400));
        // max dims 400x400 -> scale = min(300/400, 200/400) = 0.5 for both.
        assert_eq!(old.size, size(px(200.), px(100.)));
        assert_eq!(new.size, size(px(100.), px(200.)));
        assert_eq!(center(old), point(px(150.), px(100.)));
        assert_eq!(center(new), center(old));
    }

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
