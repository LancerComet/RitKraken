use crate::i18n::{Language, tr};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::Input,
        menu::{DropdownMenu as _, PopupMenuItem},
        scroll::ScrollableElement,
    },
    prelude::*,
    *,
};

use super::{PreferenceDraft, PreferencePage, Preferences};
use crate::app::{
    Client,
    widgets::{HEADER, HOVER, LINE, MUTED, PAPER, SELECT, Tone, button, error_text, heading, icon},
};

impl Client {
    pub(in crate::app) fn preferences_view(
        &self,
        draft: &PreferenceDraft,
        cx: &mut Context<Self>,
    ) -> Div {
        let sidebar_text_size = px(15.);
        let form = match draft.page {
            PreferencePage::General => self.general_preferences(draft, cx),
            PreferencePage::Appearance => self.appearance_preferences(draft, cx),
            PreferencePage::Editor => self.editor_preferences(draft, cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(PAPER))
            .child(
                div()
                    .w(px(248.))
                    .flex_shrink_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(HEADER))
                    .child(
                        button(
                            "exit-preferences",
                            tr!("app.preferences.exit"),
                            Tone::Plain,
                            true,
                            cx.listener(|this, _, window, cx| this.save_preferences(window, cx)),
                        )
                        .justify_start()
                        .h(px(41.))
                        .pl(px(14.))
                        .text_size(sidebar_text_size),
                    )
                    .child(
                        heading(tr!("app.preferences.title"))
                            .px(px(14.))
                            .py(px(11.)),
                    )
                    .children(
                        [
                            (PreferencePage::General, "icons/settings.svg"),
                            (PreferencePage::Appearance, "icons/panel-left.svg"),
                            (PreferencePage::Editor, "icons/file-diff.svg"),
                        ]
                        .into_iter()
                        .map(|(page, glyph)| {
                            let active = draft.page == page;
                            div()
                                .id(page.label_key())
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .px(px(15.))
                                .h(px(38.))
                                .cursor_pointer()
                                .bg(rgb(if active { SELECT } else { HEADER }))
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(preferences) = this.preferences.as_mut() {
                                        preferences.page = page;
                                        cx.notify();
                                    }
                                }))
                                .child(icon(glyph, MUTED))
                                .child(
                                    div()
                                        .text_size(sidebar_text_size)
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
                        .px(px(32.))
                        .py(px(19.))
                        .gap(px(14.))
                        .child(
                            div()
                                .text_size(px(23.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr!(draft.page.label_key())),
                        )
                        .when_some(draft.error.as_ref(), |body, error| {
                            body.child(error_text(error))
                        })
                        .child(form)
                        .overflow_y_scrollbar(),
                ),
            )
    }

    fn general_preferences(&self, draft: &PreferenceDraft, cx: &mut Context<Self>) -> Div {
        let options = &draft.options;
        let limited = !options.show_all_commits;
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(preference_row(
                "app.preferences.language",
                language_picker(options.language, cx),
                Some("app.preferences.language_hint"),
            ))
            .child(preference_row(
                "app.preferences.default_branch",
                Input::new(&draft.branch).w(px(255.)),
                Some("app.preferences.default_branch_hint"),
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
            ))
            .child(preference_row(
                "app.preferences.initial_commits",
                Input::new(&draft.count).w(px(255.)).disabled(!limited),
                Some("app.preferences.initial_commits_hint"),
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
            ))
    }

    fn appearance_preferences(&self, draft: &PreferenceDraft, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().gap(px(18.)).child(preference_row(
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
        ))
    }
}

fn language_picker(selected: Language, cx: &Context<Client>) -> impl IntoElement {
    let view = cx.weak_entity();
    Button::new("preferences-language")
        .outline()
        .label(selected.native_name())
        .accessibility_label(tr!("app.preferences.language"))
        .child(icon("icons/chevron-down.svg", MUTED).size(px(12.)))
        .w(px(255.))
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

pub(super) fn preference_row(
    label_key: &'static str,
    input: impl IntoElement,
    hint_key: Option<&'static str>,
) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(24.))
        .child(
            div()
                .w(px(226.))
                .flex_shrink_0()
                .pt(px(6.))
                .text_right()
                .child(tr!(label_key)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .max_w(px(560.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(input)
                .when_some(hint_key, |control, key| {
                    control.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
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
