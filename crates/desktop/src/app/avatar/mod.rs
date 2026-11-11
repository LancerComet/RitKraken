mod gravatar;

use std::{collections::HashMap, sync::Arc, time::Instant};

use gpui_kit::{
    base::{Avatar, AvatarFallback, AvatarImage},
    prelude::*,
    *,
};
use unicode_segmentation::UnicodeSegmentation as _;

use self::gravatar::{Gravatar, Key, Lookup};

const CACHE_CAPACITY: usize = 512;

#[derive(Default)]
struct AvatarCache(HashMap<Key, Instant>);

impl Global for AvatarCache {}

impl AvatarCache {
    fn touch(&mut self, key: Key) -> Option<Key> {
        let previous = self.0.insert(key, Instant::now());
        if previous.is_some() || self.0.len() <= CACHE_CAPACITY {
            return None;
        }
        let oldest = self
            .0
            .iter()
            .filter(|(other, _)| **other != key)
            .min_by_key(|(_, accessed)| *accessed)
            .map(|(key, _)| *key)?;
        self.0.remove(&oldest);
        Some(oldest)
    }
}

#[derive(gpui_kit::IntoElement)]
pub(super) struct PersonAvatar {
    key: Option<Key>,
    initial: String,
    diameter: Pixels,
    ring: Option<u32>,
}

impl PersonAvatar {
    pub(super) fn new(name: &str, email: &str, diameter: Pixels, ring: Option<u32>) -> Self {
        Self {
            key: gravatar::key(email),
            initial: author_initial(name),
            diameter,
            ring,
        }
    }
}

impl RenderOnce for PersonAvatar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let image = self.key.and_then(|key| load(key, window, cx));
        let avatar = match image {
            Some(image) => Avatar::new().image(AvatarImage::new(image).size_full().rounded_full()),
            None => Avatar::new().fallback(
                AvatarFallback::new()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(0x365479))
                    .text_size(self.diameter * 0.5)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.initial),
            ),
        };
        avatar
            .size(self.diameter)
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .bg(rgb(0xe1e8f2))
            .when_some(self.ring, |e, color| e.border_1().border_color(rgb(color)))
    }
}

fn load(key: Key, window: &mut Window, cx: &mut App) -> Option<Arc<RenderImage>> {
    if let Some(old) = cx.default_global::<AvatarCache>().touch(key) {
        let evicted = cx.fetch_asset::<Gravatar>(&old);
        cx.remove_asset::<Gravatar>(&old);
        if let Some(Lookup::Found(image)) = evicted {
            window.defer(cx, move |window, cx| cx.drop_image(image, Some(window)));
        }
    }
    match window.use_asset::<Gravatar>(&key, cx) {
        Some(Lookup::Found(image)) => Some(image),
        Some(Lookup::RetryAfter(time)) if time <= Instant::now() => {
            cx.remove_asset::<Gravatar>(&key);
            window.use_asset::<Gravatar>(&key, cx);
            None
        }
        _ => None,
    }
}

fn author_initial(name: &str) -> String {
    name.trim_start()
        .graphemes(true)
        .next()
        .unwrap_or("?")
        .to_uppercase()
        .graphemes(true)
        .next()
        .unwrap_or("?")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::author_initial;

    #[test]
    fn fallback_is_one_visible_character() {
        assert_eq!(author_initial("  lynn"), "L");
        assert_eq!(author_initial("山田"), "山");
        assert_eq!(author_initial("e\u{301}ric"), "E\u{301}");
        assert_eq!(author_initial("   "), "?");
    }
}
