use std::sync::OnceLock;

use gpui::{App, Pixels, SharedString, Window, px};
use gpui_component::ActiveTheme as _;

/// Optional overrides for the code area's font and row spacing.
#[derive(Clone, Debug, Default)]
pub struct DiffTypography {
    pub font_family: Option<SharedString>,
    pub font_size: Option<Pixels>,
    pub line_height: Option<Pixels>,
}

impl DiffTypography {
    pub(crate) fn resolve(&self, window: &Window, cx: &App) -> (SharedString, Pixels, Pixels) {
        let theme = cx.theme();
        let family = self
            .font_family
            .as_ref()
            .filter(|family| {
                font_families(cx)
                    .binary_search_by(|name| name.as_str().cmp(family.as_str()))
                    .is_ok()
            })
            .cloned()
            .unwrap_or_else(|| theme.mono_font_family.clone());
        let font_size = self
            .font_size
            .filter(|size| positive(*size))
            .unwrap_or_else(|| window.rem_size() * (f32::from(theme.mono_font_size) / 16.));
        let line_height = self
            .line_height
            .filter(|height| positive(*height))
            .unwrap_or(font_size * 1.6)
            .max(font_size);
        (family, font_size, line_height)
    }
}

fn positive(value: Pixels) -> bool {
    value > px(0.) && f32::from(value).is_finite()
}

/// Font families the platform text system can resolve in this process.
pub fn font_families(cx: &App) -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| cx.text_system().all_font_names())
}
