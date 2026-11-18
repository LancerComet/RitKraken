use crate::i18n::{Language, tr};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::Input,
        menu::{DropdownMenu as _, PopupMenuItem},
        scroll::{ScrollableElement, Scrollbar},
    },
    prelude::*,
    *,
};

use super::{PreferenceDraft, PreferencePage, Preferences, git::git_preferences};
use crate::app::{
    Client,
    theme::{Palette, ThemeId, palette},
    widgets::{Tone, button_content, error_text, icon, loading::app_logo, modal::Modal},
};

const REPOSITORY_URL: &str = "https://github.com/LancerComet/RitKraken";

impl Client {
    pub(in crate::app) fn preferences_view(
        &self,
        draft: &PreferenceDraft,
        cx: &mut Context<Self>,
    ) -> Modal {
        let p = palette(cx);
        let sidebar_text_size = px(14.);
        let form = match draft.page {
            PreferencePage::General => self.general_preferences(draft, p, cx),
            PreferencePage::Git => git_preferences(draft, cx),
            PreferencePage::Appearance => self.appearance_preferences(draft, p, cx),
            PreferencePage::Editor => self.editor_preferences(draft, cx),
            PreferencePage::About => about_preferences(p, cx),
        };
        Modal::new(
            "preferences-modal",
            tr!("app.preferences.title"),
            &self.modal_focus,
            true,
            cx.listener(|this, _, window, cx| this.save_preferences(window, cx)),
        )
        .relative_size(size(0.75, 0.8), size(px(1920.), px(1200.)))
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .flex()
                .bg(rgb(p.surface))
                .child(
                    div()
                        .w(px(214.))
                        .flex_shrink_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .border_r_1()
                        .border_color(rgb(p.border))
                        .bg(rgb(p.surface_dim))
                        .py(px(12.))
                        .px(px(10.))
                        .children(
                            [
                                (PreferencePage::General, "icons/settings.svg"),
                                (PreferencePage::Git, "icons/git-branch.svg"),
                                (PreferencePage::Appearance, "icons/panel-left.svg"),
                                (PreferencePage::Editor, "icons/file-diff.svg"),
                                (PreferencePage::About, "icons/info.svg"),
                            ]
                            .into_iter()
                            .map(|(page, glyph)| {
                                let active = draft.page == page;
                                div()
                                    .id(page.label_key())
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .px(px(14.))
                                    .h(px(38.))
                                    .rounded_full()
                                    .cursor_pointer()
                                    .bg(rgb(if active { p.accent_soft } else { p.surface_dim }))
                                    .text_color(rgb(if active { p.accent_soft_fg } else { p.text }))
                                    .hover(|s| {
                                        s.bg(rgb(if active {
                                            p.accent_soft_hover
                                        } else {
                                            p.hover
                                        }))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(preferences) = this.preferences.as_mut() {
                                            preferences.page = page;
                                            cx.notify();
                                        }
                                    }))
                                    .child(icon(
                                        glyph,
                                        if active {
                                            p.accent_soft_fg
                                        } else {
                                            p.text_muted
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_size(sidebar_text_size)
                                            .font_weight(if active {
                                                FontWeight::MEDIUM
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .child(tr!(page.label_key())),
                                    )
                            }),
                        ),
                )
                .child(
                    div().flex_1().min_w_0().h_full().child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .px(px(26.))
                            .py(px(20.))
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(p.text))
                                    .child(tr!(draft.page.label_key())),
                            )
                            .when_some(draft.error.as_ref(), |body, error| {
                                body.child(error_text(error, cx).flex_shrink_0())
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "preferences-form-{}",
                                        draft.page.label_key()
                                    )))
                                    .flex_1()
                                    .min_h_0()
                                    // Focus rings extend beyond the control's border.
                                    // Keep them inside the scrolling area's clip.
                                    .child(form.p(px(4.)).pr(Scrollbar::width() + px(8.)))
                                    .overflow_y_scrollbar(),
                            ),
                    ),
                ),
        )
    }

    fn general_preferences(
        &self,
        draft: &PreferenceDraft,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let options = &draft.options;
        let limited = !options.show_all_commits;
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(preference_row(
                "app.preferences.language",
                language_picker(options.language, p, cx),
                Some("app.preferences.language_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.default_branch",
                Input::new(&draft.branch).w(px(255.)).max_w_full().min_w_0(),
                Some("app.preferences.default_branch_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.show_all_history",
                preference_check(
                    "show-all-history",
                    "app.preferences.show_all_history",
                    options.show_all_commits,
                    true,
                    |options, checked| options.show_all_commits = checked,
                    cx,
                ),
                Some("app.preferences.show_all_history_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.initial_commits",
                Input::new(&draft.count)
                    .w(px(255.))
                    .max_w_full()
                    .min_w_0()
                    .disabled(!limited),
                Some("app.preferences.initial_commits_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.lazy_history",
                preference_check(
                    "lazy-history",
                    "app.preferences.lazy_history",
                    options.lazy_history,
                    limited,
                    |options, checked| options.lazy_history = checked,
                    cx,
                ),
                Some("app.preferences.lazy_history_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.log_file_size",
                Input::new(&draft.log_size)
                    .w(px(132.))
                    .max_w_full()
                    .min_w_0()
                    .suffix(unit_suffix("MB", cx))
                    .aria_label(tr!("app.preferences.log_file_size")),
                Some("app.preferences.log_file_size_hint"),
                p,
            ))
            .child(preference_row(
                "app.preferences.remember_tabs",
                preference_check(
                    "remember-tabs",
                    "app.preferences.remember_tabs",
                    options.remember_tabs,
                    true,
                    |options, checked| options.remember_tabs = checked,
                    cx,
                ),
                Some("app.preferences.remember_tabs_hint"),
                p,
            ))
    }

    fn appearance_preferences(
        &self,
        draft: &PreferenceDraft,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = draft.options.theme;
        let cards = ThemeId::ALL.chunks(2).map(|pair| {
            div()
                .w_full()
                .flex()
                .gap(px(10.))
                .children(pair.iter().map(|&theme| {
                    theme_card(theme, theme == selected, p, cx)
                        .flex_1()
                        .min_w_0()
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(preference_row(
                "app.preferences.theme",
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(cards),
                None,
                p,
            ))
            .child(preference_row(
                "app.preferences.collapse_review_sidebar",
                preference_check(
                    "collapse-review-sidebar",
                    "app.preferences.collapse_review_sidebar",
                    draft.options.collapse_review_sidebar,
                    true,
                    |options, checked| options.collapse_review_sidebar = checked,
                    cx,
                ),
                Some("app.preferences.collapse_review_sidebar_hint"),
                p,
            ))
    }
}

fn about_preferences(p: Palette, cx: &App) -> Div {
    let version = format!(
        "{} v{}",
        tr!("app.preferences.version"),
        env!("CARGO_PKG_VERSION")
    );
    div()
        .w_full()
        .max_w(px(520.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            div()
                .w_full()
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(18.))
                .child(img(app_logo(cx)).size(px(72.)).flex_shrink_0())
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(22.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(p.text))
                                .child("RitKraken"),
                        )
                        .child(
                            div()
                                .px(px(10.))
                                .py(px(3.))
                                .rounded_full()
                                .bg(rgb(p.accent_soft))
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(p.accent_soft_fg))
                                .child(version),
                        ),
                ),
        )
        .child(
            button_content(
                "about-github-link",
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(icon("icons/github.svg", p.text).size(px(16.)))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(p.text))
                                            .child("GitHub"),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.))
                                            .text_color(rgb(p.accent))
                                            .child(REPOSITORY_URL),
                                    ),
                            ),
                    )
                    .child(icon("icons/external-link.svg", p.text_muted).size(px(14.))),
                Tone::Plain,
                true,
                cx,
                |_, _, cx| cx.open_url(REPOSITORY_URL),
            )
            .w_full()
            .h(px(52.))
            .px(px(10.))
            .rounded(px(10.)),
        )
}

fn theme_card(theme: ThemeId, active: bool, p: Palette, cx: &mut Context<Client>) -> Stateful<Div> {
    let swatch = theme.palette();
    let mode_label = tr!(if swatch.is_dark {
        "app.preferences.theme_dark"
    } else {
        "app.preferences.theme_light"
    });
    div()
        .id(("theme-card", theme as usize))
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(14.))
        .border_1()
        .border_color(rgb(if active { p.accent } else { p.border }))
        .bg(rgb(if active { p.accent_soft } else { p.surface_dim }))
        .hover(|style| style.bg(rgb(if active { p.accent_soft_hover } else { p.hover })))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.choose_theme(theme, cx);
        }))
        .child(
            div()
                .w_full()
                .h(px(44.))
                .rounded(px(10.))
                .overflow_hidden()
                .border_1()
                .border_color(rgb(swatch.border))
                .bg(rgb(swatch.canvas))
                .flex()
                .flex_col()
                .child(
                    div()
                        .h(px(12.))
                        .w_full()
                        .bg(rgb(swatch.chrome))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(div().size(px(5.)).rounded_full().bg(rgb(swatch.danger)))
                        .child(div().size(px(5.)).rounded_full().bg(rgb(swatch.warning)))
                        .child(div().size(px(5.)).rounded_full().bg(rgb(swatch.success))),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .child(
                            div()
                                .w(px(30.))
                                .h_full()
                                .bg(rgb(swatch.surface_dim))
                                .border_r_1()
                                .border_color(rgb(swatch.border)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .p(px(5.))
                                .flex()
                                .flex_col()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .child(
                                            div()
                                                .h(px(5.))
                                                .w(px(28.))
                                                .rounded_full()
                                                .bg(rgb(swatch.accent)),
                                        )
                                        .child(
                                            div()
                                                .h(px(5.))
                                                .w(px(18.))
                                                .rounded_full()
                                                .bg(rgb(swatch.renamed)),
                                        ),
                                )
                                .child(
                                    div()
                                        .h(px(4.))
                                        .w(px(44.))
                                        .rounded_full()
                                        .bg(rgb(swatch.text_subtle)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(6.))
                .child(
                    div().flex().items_center().gap(px(6.)).min_w_0().child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if active { p.accent_soft_fg } else { p.text }))
                            .child(theme.name()),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .flex_shrink_0()
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(1.))
                                .rounded_full()
                                .bg(rgb(if active { p.surface } else { p.surface_high }))
                                .text_size(px(10.))
                                .text_color(rgb(p.text_muted))
                                .child(mode_label),
                        )
                        .when(active, |badge| {
                            badge.child(
                                div()
                                    .size(px(16.))
                                    .rounded_full()
                                    .bg(rgb(p.accent))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("icons/check.svg", p.accent_fg).size(px(11.))),
                            )
                        }),
                ),
        )
}

fn language_picker(selected: Language, p: Palette, cx: &Context<Client>) -> impl IntoElement {
    let view = cx.weak_entity();
    Button::new("preferences-language")
        .outline()
        .label(selected.native_name())
        .accessibility_label(tr!("app.preferences.language"))
        .child(icon("icons/chevron-down.svg", p.text_muted).size(px(12.)))
        .w(px(255.))
        .max_w_full()
        .min_w_0()
        .dropdown_menu(move |menu, _, _| {
            Language::ALL.into_iter().fold(menu, |menu, language| {
                let view = view.clone();
                menu.item(
                    PopupMenuItem::new(language.native_name())
                        .checked(language == selected)
                        .on_click(move |_, window, cx| {
                            let _ = view
                                .update(cx, |this, cx| this.choose_language(language, window, cx));
                        }),
                )
            })
        })
}

pub(super) fn unit_suffix(unit: impl Into<SharedString>, cx: &App) -> Div {
    let p = palette(cx);
    div().text_color(rgb(p.text_muted)).child(unit.into())
}

pub(super) fn preference_row(
    label_key: &'static str,
    input: impl IntoElement,
    hint_key: Option<&'static str>,
    p: Palette,
) -> Div {
    render_preference_row(label_key, input, hint_key, p, false)
}

pub(super) fn preference_action_row(
    label_key: &'static str,
    action: impl IntoElement,
    p: Palette,
) -> Div {
    render_preference_row(label_key, action, None, p, true)
}

fn render_preference_row(
    label_key: &'static str,
    input: impl IntoElement,
    hint_key: Option<&'static str>,
    p: Palette,
    centered: bool,
) -> Div {
    div()
        .flex()
        .items_start()
        .when(centered, |row| row.items_center())
        .gap(px(24.))
        .child(
            div()
                .w(relative(0.32))
                .max_w(px(210.))
                .min_w_0()
                .flex_shrink_0()
                .when(!centered, |label| label.pt(px(6.)))
                .text_right()
                .text_color(rgb(p.text))
                .child(tr!(label_key)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .max_w(px(580.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(input)
                .when_some(hint_key, |control, key| {
                    control.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(tr!(key)),
                    )
                }),
        )
}

pub(super) fn preference_check(
    id: &'static str,
    label_key: &'static str,
    checked: bool,
    enabled: bool,
    edit: impl Fn(&mut Preferences, bool) + 'static,
    cx: &mut Context<Client>,
) -> Checkbox {
    Checkbox::new(id)
        .accessibility_label(tr!(label_key))
        .checked(checked)
        .disabled(!enabled)
        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
            if let Some(preferences) = this.preferences.as_mut() {
                edit(&mut preferences.options, *checked);
                cx.notify();
            }
        }))
}
