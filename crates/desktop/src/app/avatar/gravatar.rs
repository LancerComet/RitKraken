use std::{
    io::Cursor,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gpui_kit::{App, Asset, RenderImage};
use image::{DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};
use tokio::{runtime, sync::Semaphore, task::JoinSet, time::timeout};

const AVATAR_SIZE: u32 = 80;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_IMAGE_SIDE: u32 = 160;
const RETRY_DELAY: Duration = Duration::from_secs(300);

static RUNTIME: OnceLock<Option<runtime::Runtime>> = OnceLock::new();
static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
static REQUESTS: Semaphore = Semaphore::const_new(6);

pub(super) type Key = [u8; 32];

pub(super) fn key(email: &str) -> Option<Key> {
    let email = email.trim();
    (!email.is_empty()).then(|| Sha256::digest(email.to_lowercase().as_bytes()).into())
}

pub(super) struct Gravatar;

#[derive(Clone)]
pub(super) enum Lookup {
    Found(Arc<RenderImage>),
    Missing,
    RetryAfter(Instant),
}

impl Asset for Gravatar {
    type Source = Key;
    type Output = Lookup;

    fn load(key: Key, _: &mut App) -> impl Future<Output = Lookup> + Send + 'static {
        async move {
            match download(key).await {
                Some(Download::Image(bytes)) => {
                    decode(bytes).map_or(Lookup::Missing, Lookup::Found)
                }
                Some(Download::Missing) => Lookup::Missing,
                None => Lookup::RetryAfter(Instant::now() + RETRY_DELAY),
            }
        }
    }
}

enum Download {
    Image(Vec<u8>),
    Missing,
}

async fn download(key: Key) -> Option<Download> {
    let runtime = RUNTIME
        .get_or_init(|| {
            runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("ritkraken-avatars")
                .enable_all()
                .build()
                .ok()
        })
        .as_ref()?;
    // Dropping the GPUI asset load also cancels its queued or active request.
    let mut tasks = JoinSet::new();
    tasks.spawn_on(request(key), runtime.handle());
    tasks.join_next().await?.ok()?
}

async fn request(key: Key) -> Option<Download> {
    let _permit = timeout(Duration::from_secs(10), REQUESTS.acquire())
        .await
        .ok()?
        .ok()?;
    let client = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .https_only(true)
                .redirect_policy(reqwest::redirect::Policy::limited(3))
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(5))
                .build()
                .ok()
        })
        .as_ref()?;
    let mut response = client
        .get(format!(
            "https://gravatar.com/avatar/{}?s={AVATAR_SIZE}&d=404&r=g",
            hex::encode(key)
        ))
        .send()
        .await
        .ok()?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Some(Download::Missing);
    }
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(Download::Image(bytes))
}

fn decode(bytes: Vec<u8>) -> Option<Arc<RenderImage>> {
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
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(2 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let mut pixels = image.into_rgba8();
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([Frame::new(pixels)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_the_normalized_email() {
        assert_eq!(
            hex::encode(key("  MyEmailAddress@example.com  ").unwrap()),
            "84059b07d4be67b806386c0aad8070a23f18836bbaae342275dc0a83414c32ee",
        );
        assert_eq!(key(" \t "), None);
    }

    #[test]
    fn renders_image_data_as_bgra_and_rejects_oversize_images() {
        let png = |width| {
            let rgba = image::RgbaImage::from_pixel(width, 1, image::Rgba([250, 2, 10, 255]));
            let mut bytes = Cursor::new(Vec::new());
            rgba.write_to(&mut bytes, ImageFormat::Png).unwrap();
            bytes.into_inner()
        };
        let avatar = decode(png(1)).unwrap();
        assert_eq!(avatar.as_bytes(0), Some([10, 2, 250, 255].as_slice()));
        assert!(decode(png(MAX_IMAGE_SIDE + 1)).is_none());
    }
}
