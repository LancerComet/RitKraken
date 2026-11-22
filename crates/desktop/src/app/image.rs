use std::{io::Cursor, sync::Arc};

use gpui_kit::RenderImage;
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
