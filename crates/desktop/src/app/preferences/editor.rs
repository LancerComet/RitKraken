use std::ops::RangeInclusive;

use gpui_kit::{
    component::{
        IndexPath,
        input::{Input, InputState, NumberInput},
        select::{SearchableVec, Select, SelectItem, SelectState},
    },
    prelude::*,
    *,
};
use ritkraken_diff::{DiffFile, DiffState, DiffTypography, font_families};
use serde_json::Value;

use super::{
    PreferenceDraft, Preferences, unsigned_input,
    view::{preference_check, preference_row, unit_suffix},
};
use crate::{
    app::{
        Client, Message,
        review::DiffLayout,
        theme::palette,
        widgets::heading,
    },
    i18n::{LocalizedInput, tr},
};

const FONT_SIZE: RangeInclusive<u16> = 8..=48;
const MAX_LINE_HEIGHT: u16 = 96;
const INVALID_FONT_SIZE: &str = "app.error.editor_font_size";
const INVALID_LINE_HEIGHT: &str = "app.error.editor_line_height";

const PREVIEW_PATCH: &str = concat!(
    r#"--- a/preview.rs
+++ b/preview.rs
@@ -1,9 +1,10 @@
 fn greet(name: &str) -> String {
-    let message = format!("Hello {name}");
+    let message = format!("Hello, {name}!");
+    // Unicode sample: 中文・日本語
     println!("{message}");
     message
 }
"#,
    " \n",
    r#" fn main() {
     greet("RitKraken");
 }
"#
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct EditorTypography {
    font_family: Option<String>,
    font_size: u16,
    line_height: Option<u16>,
}

impl Default for EditorTypography {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 12,
            line_height: Some(18),
        }
    }
}

impl EditorTypography {
    pub(super) fn read(value: &Value) -> Self {
        let defaults = Self::default();
        let read_px = |key: &str| value[key].as_u64().and_then(|px| u16::try_from(px).ok());
        let font_size = read_px("font_size")
            .filter(|size| FONT_SIZE.contains(size))
            .unwrap_or(defaults.font_size);
        Self {
            font_family: value["font_family"]
                .as_str()
                .map(str::trim)
                .filter(|family| !family.is_empty())
                .map(str::to_owned),
            font_size,
            line_height: value
                .get("line_height")
                .map_or(defaults.line_height, |_| read_px("line_height"))
                .filter(|height| (font_size..=MAX_LINE_HEIGHT).contains(height)),
        }
    }

    pub(super) fn json(&self) -> Value {
        serde_json::json!({
            "font_family": self.font_family,
            "font_size": self.font_size,
            "line_height": self.line_height,
        })
    }

    pub(in crate::app) fn diff_typography(&self) -> DiffTypography {
        DiffTypography {
            font_family: self.font_family.as_deref().map(SharedString::from),
            font_size: Some(px(f32::from(self.font_size))),
            line_height: self.line_height.map(|height| px(f32::from(height))),
        }
    }

    fn from_inputs(
        font_family: Option<String>,
        font_size: &str,
        line_height: &str,
    ) -> Result<Self, &'static str> {
        let font_size = font_size
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|size| FONT_SIZE.contains(size))
            .ok_or(INVALID_FONT_SIZE)?;
        let line_height = line_height.trim();
        let line_height = if line_height.is_empty() {
            None
        } else {
            Some(
                line_height
                    .parse::<u16>()
                    .ok()
                    .filter(|height| (font_size..=MAX_LINE_HEIGHT).contains(height))
                    .ok_or(INVALID_LINE_HEIGHT)?,
            )
        };
        Ok(Self {
            font_family,
            font_size,
            line_height,
        })
    }
}

#[derive(Clone)]
struct FontChoice(Option<String>);

impl SelectItem for FontChoice {
    type Value = Option<String>;

    fn title(&self) -> SharedString {
        self.0
            .as_deref()
            .map(SharedString::from)
            .unwrap_or_else(|| tr!("app.preferences.editor_font_default").into())
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub(super) struct EditorDraft {
    font: Entity<SelectState<SearchableVec<FontChoice>>>,
    font_size: Entity<InputState>,
    line_height: LocalizedInput,
    preview: Entity<DiffState>,
    _observers: [Subscription; 3],
}

impl EditorDraft {
    pub(super) fn new(
        options: &Preferences,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) -> Self {
        let choices: Vec<_> = std::iter::once(FontChoice(None))
            .chain(
                font_families(cx)
                    .iter()
                    .cloned()
                    .map(|family| FontChoice(Some(family))),
            )
            .collect();
        let selected = choices
            .iter()
            .position(|choice| choice.0 == options.editor.font_family)
            .unwrap_or(0);
        let font = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(choices),
                Some(IndexPath::default().row(selected)),
                window,
                cx,
            )
            .searchable(true)
        });
        let font_size = cx.new(|cx| {
            unsigned_input(Some(options.editor.font_size), window, cx)
                .min(f64::from(*FONT_SIZE.start()))
                .max(f64::from(*FONT_SIZE.end()))
        });
        let line_height =
            LocalizedInput::new("app.preferences.editor_line_height_auto", cx, |cx| {
                unsigned_input(options.editor.line_height, window, cx)
            });
        let preview = cx.new(|cx| {
            let files = DiffFile::parse(PREVIEW_PATCH).expect("valid editor preview patch");
            let mut state = DiffState::new(files, cx);
            options.diff_layout.apply(&mut state, cx);
            state
        });
        let _observers = [
            cx.observe(&font, |this, _, cx| this.refresh_editor_preview(cx)),
            cx.observe(&font_size, |this, _, cx| this.refresh_editor_preview(cx)),
            cx.observe(&line_height, |this, _, cx| this.refresh_editor_preview(cx)),
        ];
        Self {
            font,
            font_size,
            line_height,
            preview,
            _observers,
        }
    }

    pub(super) fn typography(&self, cx: &App) -> Result<EditorTypography, &'static str> {
        let family = self.font.read(cx).selected_value().cloned().flatten();
        EditorTypography::from_inputs(
            family,
            self.font_size.read(cx).value().as_str(),
            self.line_height.read(cx).value().as_str(),
        )
    }

    pub(super) fn localize(&self, window: &mut Window, cx: &mut Context<Client>) {
        self.line_height.localize(window, cx);
    }
}

impl Client {
    fn refresh_editor_preview(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.preferences.as_mut() else {
            return;
        };
        let Ok(typography) = draft.editor.typography(cx) else {
            return;
        };
        let has_editor_error = matches!(
            draft.error,
            Some(Message::Key(INVALID_FONT_SIZE | INVALID_LINE_HEIGHT))
        );
        if draft.options.editor != typography || has_editor_error {
            draft.options.editor = typography;
            if has_editor_error {
                draft.error = None;
            }
            cx.notify();
        }
    }

    pub(super) fn editor_preferences(
        &self,
        draft: &PreferenceDraft,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let options = &draft.options;
        let editor = &draft.editor;
        let layouts = div()
            .p(px(3.))
            .rounded_full()
            .bg(rgb(p.surface_dim))
            .border_1()
            .border_color(rgb(p.border))
            .flex()
            .gap(px(2.))
            .self_start()
            .children(DiffLayout::ALL.into_iter().map(|layout| {
                layout
                    .selector_button(
                        Some(options.diff_layout),
                        true,
                        cx,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(draft) = this.preferences.as_mut() {
                                draft.options.diff_layout = layout;
                                draft
                                    .editor
                                    .preview
                                    .update(cx, |state, cx| layout.apply(state, cx));
                                cx.notify();
                            }
                        }),
                    )
                    .rounded_full()
                    .w(px(74.))
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(preference_row(
                "app.preferences.diff_layout",
                layouts,
                Some("app.preferences.diff_layout_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.wrap_lines",
                preference_check(
                    "default-wrap",
                    "app.preferences.wrap_lines",
                    options.wrap_lines,
                    true,
                    |options, checked| options.wrap_lines = checked,
                    cx,
                ),
                Some("app.preferences.wrap_lines_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.editor_font",
                Select::new(&editor.font)
                    .w(px(290.))
                    .max_w_full()
                    .min_w_0()
                    .menu_max_h(px(320.))
                    .search_placeholder(tr!("app.preferences.editor_font_search"))
                    .accessibility_label(tr!("app.preferences.editor_font")),
                None,
                p,
            ))
            .child(preference_row(
                "app.preferences.editor_font_size",
                NumberInput::new(&editor.font_size)
                    .w(px(132.))
                    .max_w_full()
                    .min_w_0()
                    .suffix(unit_suffix("px", cx))
                    .accessibility_label(tr!("app.preferences.editor_font_size")),
                None,
                p,
            ))
            .child(preference_row(
                "app.preferences.editor_line_height",
                Input::new(&editor.line_height)
                    .w(px(132.))
                    .max_w_full()
                    .min_w_0()
                    .suffix(unit_suffix("px", cx))
                    .aria_label(tr!("app.preferences.editor_line_height")),
                Some("app.preferences.editor_line_height_hint"),
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .max_w(px(940.))
                    .child(heading(tr!("app.preferences.editor_preview"), cx).text_size(px(13.)))
                    .child(
                        div()
                            .id("editor-preview")
                            .h(px(244.))
                            .flex_shrink_0()
                            .overflow_hidden()
                            .rounded(px(12.))
                            .border_1()
                            .border_color(rgb(p.border))
                            .bg(rgb(p.canvas))
                            .child(options.diff_layout.viewer(
                                &editor.preview,
                                options.wrap_lines,
                                options.editor.diff_typography(),
                            )),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiffFile, EditorTypography, INVALID_FONT_SIZE, INVALID_LINE_HEIGHT, PREVIEW_PATCH,
    };

    #[test]
    fn editor_typography_keeps_legacy_defaults_and_round_trips() {
        assert_eq!(
            EditorTypography::read(&serde_json::Value::Null),
            EditorTypography::default()
        );
        let options = EditorTypography::from_inputs(Some("Consolas".into()), "18", "29").unwrap();
        assert_eq!(EditorTypography::read(&options.json()), options);
        assert_eq!(
            EditorTypography::from_inputs(None, "18", "")
                .unwrap()
                .line_height,
            None
        );
        assert_eq!(
            EditorTypography::from_inputs(None, "18", "17"),
            Err(INVALID_LINE_HEIGHT)
        );
        assert_eq!(
            EditorTypography::from_inputs(None, "49", ""),
            Err(INVALID_FONT_SIZE)
        );
    }

    #[test]
    fn preview_contains_a_real_diff() {
        let files = DiffFile::parse(PREVIEW_PATCH).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].additions(), 2);
        assert_eq!(files[0].deletions(), 1);
    }
}
