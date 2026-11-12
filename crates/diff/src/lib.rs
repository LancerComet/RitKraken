//! RitKraken's viewport and change controls for the pinned GPUI Diff.

use gpui::App;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, StyledExt, button, h_flex, input, scroll, v_flex,
};

#[doc(hidden)]
pub use gpui_component::_rust_i18n_try_translate;

macro_rules! _rust_i18n_t {
    ($($tokens:tt)*) => {
        rust_i18n::_tr!($($tokens)*)
    };
}
pub(crate) use _rust_i18n_t;

mod controls;
mod horizontal;
mod overview;
mod split;
mod typography;
include!(concat!(env!("OUT_DIR"), "/modules.rs"));
use language_name::language_name;

mod highlighter {
    use std::ops::Range;

    use gpui::SharedString;

    pub(crate) use crate::language_name::language_name;

    pub(crate) struct SyntaxHighlighter(SharedString);

    impl SyntaxHighlighter {
        pub(crate) fn new(language: &str) -> Self {
            Self(language.into())
        }

        pub(crate) fn capture_names(
            &mut self,
            text: &crate::Rope,
        ) -> Vec<(Range<usize>, SharedString)> {
            crate::syntect_backend::capture_names(&self.0, &text.to_string())
        }
    }
}

use gpui_component::Rope;

pub use diff::{
    Diff, DiffFile, DiffHunkSeparator, DiffLinePosition, DiffMode, DiffParseOptions, DiffSide,
    DiffState,
};
pub use overview::{DiffChangeKind, DiffOverview, DiffOverviewSpan};
pub use typography::{DiffTypography, font_families};

pub fn init(cx: &mut App) {
    diff::init(cx);
}
