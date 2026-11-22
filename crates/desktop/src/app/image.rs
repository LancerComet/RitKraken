use std::{io::Cursor, sync::Arc};

use gpui_kit::{RenderImage, SMOOTH_SVG_SCALE_FACTOR, SvgRenderer};
use image::{DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Limits};

pub(in crate::app) struct DecodedImage {
    pub image: Arc<RenderImage>,
    pub width: u32,
    pub height: u32,
}

pub(in crate::app) fn decode_image(
    bytes: &[u8],
    max_side: u32,
    max_alloc: u64,
) -> Option<DecodedImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Gif | ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(max_side);
    limits.max_image_height = Some(max_side);
    limits.max_alloc = Some(max_alloc);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let mut pixels = image.into_rgba8();
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let (width, height) = pixels.dimensions();
    Some(DecodedImage {
        image: Arc::new(RenderImage::new([Frame::new(pixels)])),
        width,
        height,
    })
}

pub(in crate::app) fn decode_svg(bytes: &[u8], renderer: &SvgRenderer) -> Option<DecodedImage> {
    let parsed = renderer.parse_svg(bytes).ok()?;
    let image = renderer.render_parsed(&parsed, 1.0_f32).ok()?;
    let logical = |pixels| (i32::from(pixels) as f32 / SMOOTH_SVG_SCALE_FACTOR).round() as u32;
    let size = image.size(0);
    Some(DecodedImage {
        image,
        width: logical(size.width),
        height: logical(size.height),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::SvgRenderer;

    use super::decode_svg;

    #[test]
    fn svg_reports_its_logical_size() {
        let renderer = SvgRenderer::new(Arc::new(()));
        let decoded = decode_svg(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"/>"#,
            &renderer,
        )
        .expect("a valid SVG must decode");
        assert_eq!((decoded.width, decoded.height), (10, 20));
    }

    #[test]
    fn invalid_svg_returns_none() {
        let renderer = SvgRenderer::new(Arc::new(()));
        assert!(decode_svg(b"not an svg", &renderer).is_none());
        assert!(decode_svg(&[], &renderer).is_none());
    }
}
