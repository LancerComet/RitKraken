mod palettes;

use std::rc::Rc;

use gpui_kit::{
    component::{Theme, ThemeConfig},
    *,
};

pub(in crate::app) use palettes::SyntaxColors;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum ThemeId {
    #[default]
    ObsidianBento,
    Md3Expressive,
    Classic,
    FluentLight,
}

impl ThemeId {
    pub(in crate::app) const ALL: [Self; 4] = [
        Self::ObsidianBento,
        Self::Md3Expressive,
        Self::Classic,
        Self::FluentLight,
    ];

    pub(in crate::app) fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::ObsidianBento => "obsidian_bento",
            Self::Md3Expressive => "md3_expressive",
            Self::FluentLight => "fluent_light",
        }
    }

    pub(in crate::app) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.id() == id)
    }

    pub(in crate::app) fn default_for_appearance(appearance: WindowAppearance) -> Self {
        let is_dark = matches!(
            appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        Self::ALL
            .into_iter()
            .find(|theme| theme.palette().is_dark == is_dark)
            .unwrap_or_default()
    }

    pub(in crate::app) fn name(self) -> &'static str {
        match self {
            Self::Classic => "Modern Classic",
            Self::ObsidianBento => "Obsidian Bento",
            Self::Md3Expressive => "MD3 Expressive",
            Self::FluentLight => "Fluent Light",
        }
    }

    pub(in crate::app) fn palette(self) -> Palette {
        self.spec().palette
    }

    fn spec(self) -> ThemeSpec {
        match self {
            Self::Classic => palettes::classic(),
            Self::ObsidianBento => palettes::obsidian_bento(),
            Self::Md3Expressive => palettes::md3_expressive(),
            Self::FluentLight => palettes::fluent_light(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct WaveLayerPalette {
    pub left: u32,
    pub right: u32,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct Palette {
    pub is_dark: bool,
    pub shell_bg: u32,
    pub canvas: u32,
    pub surface: u32,
    pub surface_dim: u32,
    pub surface_high: u32,
    pub chrome_alt: u32,
    pub chrome_hover: u32,
    pub chrome_border: u32,
    pub chrome_text: u32,
    pub chrome_muted: u32,
    pub border: u32,
    pub border_strong: u32,
    pub text: u32,
    pub text_muted: u32,
    pub text_subtle: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_fg: u32,
    pub accent_soft: u32,
    pub accent_soft_fg: u32,
    pub accent_soft_border: u32,
    pub accent_soft_hover: u32,
    pub hover: u32,
    pub success: u32,
    pub success_bg: u32,
    pub success_border: u32,
    pub success_hover: u32,
    pub danger: u32,
    pub danger_bg: u32,
    pub danger_border: u32,
    pub danger_hover: u32,
    pub warning: u32,
    pub warning_bg: u32,
    pub renamed: u32,
    pub info_bg: u32,
    pub info_fg: u32,
    pub avatar_bg: u32,
    pub avatar_fg: u32,
    pub graph_rails: [u32; 9],
    pub wave_layers: [WaveLayerPalette; 3],
}

impl Palette {
    pub(in crate::app) fn rail_color(self, index: usize) -> u32 {
        self.graph_rails[index % self.graph_rails.len()]
    }
}

pub(super) struct ThemeSpec {
    pub palette: Palette,
    pub syntax: SyntaxColors,
}

#[derive(Clone, Copy)]
struct AppTheme {
    id: ThemeId,
    palette: Palette,
}

impl Global for AppTheme {}

pub(in crate::app) fn init(cx: &mut App) {
    apply_theme(ThemeId::default(), cx);
}

pub(in crate::app) fn resolve_theme(
    selected: Option<ThemeId>,
    appearance: WindowAppearance,
) -> ThemeId {
    selected.unwrap_or_else(|| ThemeId::default_for_appearance(appearance))
}

pub(in crate::app) fn active_theme(cx: &App) -> ThemeId {
    cx.try_global::<AppTheme>()
        .map_or_else(ThemeId::default, |theme| theme.id)
}

pub(in crate::app) fn palette(cx: &App) -> Palette {
    cx.try_global::<AppTheme>()
        .map_or_else(|| ThemeId::default().palette(), |theme| theme.palette)
}

pub(in crate::app) fn apply_theme(id: ThemeId, cx: &mut App) {
    let spec = id.spec();
    cx.set_global(AppTheme {
        id,
        palette: spec.palette,
    });
    let config = Rc::new(build_theme_config(id, spec));
    Theme::update(cx, |theme| {
        theme.apply_config(&config);
        theme.radius = px(8.);
        theme.radius_lg = px(14.);
        theme.shadow = true;
        theme.notification.max_items = 4;
        theme.notification.width = px(400.);
        theme.notification.margins.top = px(102.);
    });
}

fn hex(color: u32) -> String {
    format!("#{color:06x}")
}

fn hex_alpha(color: u32, alpha: u8) -> String {
    format!("#{color:06x}{alpha:02x}")
}

fn build_theme_config(id: ThemeId, spec: ThemeSpec) -> ThemeConfig {
    let p = spec.palette;
    let s = spec.syntax;
    let mode = if p.is_dark { "dark" } else { "light" };
    let colors: serde_json::Map<String, serde_json::Value> = [
        ("background", hex(p.canvas)),
        ("foreground", hex(p.text)),
        ("border", hex(p.border)),
        ("input.border", hex(p.border_strong)),
        ("ring", hex(p.accent)),
        ("caret", hex(p.accent)),
        ("primary.background", hex(p.accent)),
        ("primary.foreground", hex(p.accent_fg)),
        ("primary.hover.background", hex(p.accent_hover)),
        ("primary.active.background", hex(p.accent_hover)),
        ("secondary.background", hex(p.surface_high)),
        ("secondary.foreground", hex(p.text)),
        ("secondary.hover.background", hex(p.hover)),
        ("secondary.active.background", hex(p.accent_soft)),
        ("accent.background", hex(p.accent_soft)),
        ("accent.foreground", hex(p.accent_soft_fg)),
        ("muted.background", hex(p.surface_dim)),
        ("muted.foreground", hex(p.text_muted)),
        ("popover.background", hex(p.surface)),
        ("popover.foreground", hex(p.text)),
        ("list.background", hex(p.canvas)),
        ("list.even.background", hex(p.surface_dim)),
        ("list.head.background", hex(p.surface_high)),
        ("list.hover.background", hex(p.hover)),
        ("list.active.background", hex_alpha(p.accent, 0x2e)),
        ("list.active.border", hex(p.accent)),
        ("selection.background", hex_alpha(p.accent, 0x45)),
        ("sidebar.background", hex(p.canvas)),
        ("sidebar.foreground", hex(p.text)),
        ("sidebar.border", hex(p.border)),
        ("sidebar.accent.background", hex(p.accent_soft)),
        ("sidebar.accent.foreground", hex(p.accent_soft_fg)),
        ("sidebar.primary.background", hex(p.accent)),
        ("sidebar.primary.foreground", hex(p.accent_fg)),
        ("scrollbar.background", hex_alpha(p.surface_dim, 0x00)),
        ("scrollbar.thumb.background", hex_alpha(p.text_subtle, 0x99)),
        (
            "scrollbar.thumb.hover.background",
            hex_alpha(p.text_muted, 0xcc),
        ),
        ("drag.border", hex(p.accent)),
        ("drop_target.background", hex_alpha(p.accent, 0x33)),
        ("danger.background", hex(p.danger)),
        ("danger.foreground", hex(p.accent_fg)),
        ("success.background", hex(p.success)),
        ("success.foreground", hex(p.accent_fg)),
        ("warning.background", hex(p.warning)),
        ("warning.foreground", hex(p.accent_fg)),
        ("info.background", hex(p.accent)),
        ("info.foreground", hex(p.accent_fg)),
        ("switch.background", hex(p.border_strong)),
        ("switch.thumb.background", hex(p.surface)),
        (
            "overlay",
            hex_alpha(0x000000, if p.is_dark { 0x66 } else { 0x44 }),
        ),
        ("base.red", hex(p.danger)),
        ("base.green", hex(p.success)),
        ("base.blue", hex(p.accent)),
        ("base.yellow", hex(p.warning)),
        ("base.magenta", hex(p.renamed)),
        ("base.cyan", hex(p.accent)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), serde_json::Value::String(v)))
    .collect();
    let mut syntax: serde_json::Map<String, serde_json::Value> = [
        ("attribute", s.attribute),
        ("boolean", s.constant),
        ("constant", s.constant),
        ("constructor", s.type_name),
        ("embedded", p.text),
        ("function", s.function),
        ("keyword", s.keyword),
        ("link_text", s.function),
        ("number", s.constant),
        ("operator", s.operator),
        ("property", s.property),
        ("string", s.string),
        ("string.escape", s.type_name),
        ("string.regex", s.type_name),
        ("string.special", s.constant),
        ("string.special.symbol", s.constant),
        ("tag", s.variable),
        ("text.literal", s.string),
        ("text.code.span", s.string),
        ("type", s.type_name),
        ("variable", s.variable),
        ("variable.special", s.keyword),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), serde_json::json!({ "color": hex(v) })))
    .collect();
    syntax.insert(
        "comment".into(),
        serde_json::json!({ "color": hex(s.comment), "font_style": "italic" }),
    );
    syntax.insert(
        "comment.doc".into(),
        serde_json::json!({ "color": hex(s.comment), "font_style": "italic" }),
    );
    syntax.insert(
        "emphasis".into(),
        serde_json::json!({ "font_style": "italic" }),
    );
    syntax.insert(
        "emphasis.strong".into(),
        serde_json::json!({ "font_weight": 700 }),
    );
    syntax.insert(
        "link_uri".into(),
        serde_json::json!({ "color": hex(s.type_name), "font_style": "italic" }),
    );
    syntax.insert(
        "title".into(),
        serde_json::json!({ "color": hex(s.type_name), "font_weight": 600 }),
    );
    let highlight = serde_json::json!({
        "editor.foreground": hex(p.text),
        "editor.background": hex(p.canvas),
        "editor.active_line.background": hex(p.hover),
        "editor.line_number": hex(p.text_subtle),
        "editor.active_line_number": hex(p.text),
        "editor.invisible": hex_alpha(p.text_subtle, 0x66),
        "conflict": hex(p.warning),
        "created": hex(p.success),
        "created.background": hex(p.success_bg),
        "deleted": hex(p.danger),
        "deleted.background": hex(p.danger_bg),
        "hidden": hex(p.text_muted),
        "hint": hex(p.renamed),
        "modified": hex(p.warning),
        "modified.background": hex(p.warning_bg),
        "predictive": hex(p.text_subtle),
        "warning": hex(p.warning),
        "syntax": syntax,
    });
    let value = serde_json::json!({
        "name": id.name(),
        "mode": mode,
        "radius": 8,
        "radius.lg": 14,
        "shadow": true,
        "colors": colors,
        "highlight": highlight,
    });
    serde_json::from_value(value).expect("built-in theme config must be valid")
}

#[cfg(test)]
mod tests {
    use gpui_kit::WindowAppearance;

    use super::{ThemeId, build_theme_config, resolve_theme};

    #[test]
    fn all_builtin_themes_produce_valid_configs() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_id(id.id()), Some(id));
            let config = build_theme_config(id, id.spec());
            assert_eq!(config.name.as_ref(), id.name());
            assert!(config.highlight.is_some());
        }
        assert_eq!(ThemeId::from_id("unknown-theme"), None);
    }

    #[test]
    fn resolve_theme_follows_system_appearance_until_user_chooses() {
        for dark in [WindowAppearance::Dark, WindowAppearance::VibrantDark] {
            assert_eq!(resolve_theme(None, dark), ThemeId::ObsidianBento);
        }
        for light in [WindowAppearance::Light, WindowAppearance::VibrantLight] {
            assert_eq!(resolve_theme(None, light), ThemeId::Classic);
        }
        for theme in ThemeId::ALL {
            for appearance in [
                WindowAppearance::Dark,
                WindowAppearance::VibrantDark,
                WindowAppearance::Light,
                WindowAppearance::VibrantLight,
            ] {
                assert_eq!(resolve_theme(Some(theme), appearance), theme);
            }
        }
    }

    #[test]
    fn all_builtin_themes_have_distinct_wave_palettes() {
        for (index, left) in ThemeId::ALL.iter().enumerate() {
            let left_layers = left.palette().wave_layers;
            assert!(left_layers.iter().all(|layer| layer.alpha > 0));
            for right in &ThemeId::ALL[index + 1..] {
                assert_ne!(left_layers, right.palette().wave_layers);
            }
        }
    }
}
