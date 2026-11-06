use gpui_kit::component::{Disableable, checkbox::Checkbox};

use gpui_kit::{
    component::{input::Input, scroll::ScrollableElement},
    prelude::*,
    *,
};

use super::{PreferenceDraft, PreferencePage, Preferences};
use crate::app::{
    Client,
    review::DiffLayout,
    widgets::{HEADER, HOVER, LINE, MUTED, PAPER, SELECT, Tone, button, error_text, heading, icon},
};

impl Client {
    pub(in crate::app) fn preferences_view(
        &self,
        draft: &PreferenceDraft,
        cx: &mut Context<Self>,
    ) -> Div {
        let (title, form) = match draft.page {
            PreferencePage::General => ("General", self.general_preferences(draft, cx)),
            PreferencePage::Appearance => {
                ("UI Customization", self.appearance_preferences(draft, cx))
            }
            PreferencePage::Editor => ("Editor", self.editor_preferences(draft, cx)),
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
                            "‹  Exit Preferences",
                            Tone::Plain,
                            true,
                            cx.listener(|this, _, window, cx| this.save_preferences(window, cx)),
                        )
                        .justify_start()
                        .h(px(41.))
                        .pl(px(14.))
                        .text_size(px(16.)),
                    )
                    .child(heading("PREFERENCES").px(px(14.)).py(px(11.)))
                    .children(
                        [
                            (PreferencePage::General, "General", "icons/settings.svg"),
                            (
                                PreferencePage::Appearance,
                                "UI Customization",
                                "icons/panel-left.svg",
                            ),
                            (PreferencePage::Editor, "Editor", "icons/file-diff.svg"),
                        ]
                        .into_iter()
                        .map(|(page, label, glyph)| {
                            let active = draft.page == page;
                            div()
                                .id(label)
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
                                .child(div().text_size(px(15.)).child(label))
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
                                .child(title),
                        )
                        .when_some(draft.error.as_deref(), |body, error| {
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
        div().flex().flex_col().gap(px(18.))
            .child(preference_row("Default Branch Name",
                Input::new(&draft.branch).w(px(255.)),
                Some("Sets the initial branch for new repositories. Leave blank to use the Git configuration.")))
            .child(preference_row("Show All Commits in Graph",
                preference_check("show-all-history", "Show All Commits in Graph", options.show_all_commits, true,
                    |options, checked| options.show_all_commits = checked, cx),
                Some("Loads the full reachable history when the repository is opened.")))
            .child(preference_row("Initial Commits in Graph",
                Input::new(&draft.count).w(px(255.)).disabled(!limited),
                Some("Minimum 500. This is the first batch loaded into the Commit Graph.")))
            .child(preference_row("Lazy Load Commits in Graph",
                preference_check("lazy-history", "Lazy Load Commits in Graph", options.lazy_history, limited,
                    |options, checked| options.lazy_history = checked, cx),
                Some("Loads older commits when you reach the end of the visible history.")))
            .child(preference_row("Remember tabs",
                preference_check("remember-tabs", "Remember tabs", options.remember_tabs, true,
                    |options, checked| options.remember_tabs = checked, cx),
                Some("Restores open repositories and the selected tab on the next launch.")))
    }

    fn appearance_preferences(&self, draft: &PreferenceDraft, cx: &mut Context<Self>) -> Div {
        div().flex().flex_col().gap(px(18.))
            .child(preference_row("Collapse sidebar in File View",
                preference_check("collapse-review-sidebar", "Collapse sidebar in File View", draft.options.collapse_review_sidebar, true,
                    |options, checked| options.collapse_review_sidebar = checked, cx),
                Some("Shows the compact branch rail when opening a file. The previous sidebar is restored when the file closes.")))
    }

    fn editor_preferences(&self, draft: &PreferenceDraft, cx: &mut Context<Self>) -> Div {
        let options = &draft.options;
        let layouts = div()
            .flex()
            .children(DiffLayout::ALL.into_iter().map(|layout| {
                layout
                    .selector_button(
                        Some(options.diff_layout),
                        true,
                        cx.listener(move |this, _, _, cx| {
                            if let Some(preferences) = this.preferences.as_mut() {
                                preferences.options.diff_layout = layout;
                                cx.notify();
                            }
                        }),
                    )
                    .w(px(72.))
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(preference_row(
                "Default Diff Layout",
                layouts,
                Some("Choose a compact hunk view, a full inline diff, or two panes."),
            ))
            .child(preference_row(
                "Wrap long lines",
                preference_check(
                    "default-wrap",
                    "Wrap long lines",
                    options.wrap_lines,
                    true,
                    |options, checked| options.wrap_lines = checked,
                    cx,
                ),
                Some("Wraps long lines in file previews and diffs."),
            ))
    }
}

fn preference_row(label: &'static str, input: impl IntoElement, hint: Option<&'static str>) -> Div {
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
                .child(label),
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
                .when_some(hint, |control, hint| {
                    control.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(hint))
                }),
        )
}

fn preference_check(
    id: &'static str,
    label: &'static str,
    checked: bool,
    enabled: bool,
    edit: impl Fn(&mut Preferences, bool) + 'static,
    cx: &mut Context<Client>,
) -> Checkbox {
    Checkbox::new(id)
        .accessibility_label(label)
        .checked(checked)
        .disabled(!enabled)
        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
            if let Some(preferences) = this.preferences.as_mut() {
                edit(&mut preferences.options, *checked);
                cx.notify();
            }
        }))
}
