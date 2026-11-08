use gpui_kit::{base::TextSelection, prelude::*, *};

use crate::i18n::tr;

use super::{
    Client, Escape, OpenPreferences, OpenRepos, Refresh, Submit, Tab,
    message::branch_label,
    widgets::{
        GREEN, LINE, MUTED, NAV, NAV_TEXT, PAPER, RED, TEXT, TOOLBAR, Tone, WHITE, button, heading,
        icon,
    },
};

impl Render for Client {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.session.active_tab();
        let body = if let Some(Tab::Ready(tab)) = tab {
            self.workspace(tab, window, cx).into_any_element()
        } else if let Some(Tab::Saved(tab)) = tab {
            self.saved_tab_view(tab, cx).into_any_element()
        } else {
            self.welcome(cx).into_any_element()
        };
        div()
            .id("ritkraken")
            .track_focus(&self.focus)
            .key_context("RitKraken")
            .on_action(cx.listener(|this, _: &OpenRepos, window, cx| this.open_manager(window, cx)))
            .on_action(cx.listener(|this, _: &OpenPreferences, window, cx| {
                this.open_preferences(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Refresh, window, cx| {
                if this.manager.mode.is_none()
                    && this.preferences.is_none()
                    && !this.branch_dialog.open
                {
                    this.refresh(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(WHITE))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(finish_released_text_selection())
            .child(self.tab_bar(cx))
            .child(self.toolbar(tab, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(body),
            )
            .child(self.status_bar(tab))
            .when_some(self.preferences.as_ref(), |element, preferences| {
                element.child(self.preferences_view(preferences, cx))
            })
            .when_some(self.manager.mode, |element, mode| {
                element.child(self.manager_view(mode, window, cx))
            })
            .when(self.branch_dialog.open, |element| {
                element.child(self.branch_view(cx))
            })
    }
}

fn finish_released_text_selection() -> Canvas<()> {
    canvas(
        |_, _, _| (),
        |_, _, window, _| {
            window.on_mouse_event(|event: &MouseMoveEvent, phase, window, cx| {
                // Native resizing can consume the button-up. End the drag before
                // GPUI's selection layer handles the next move in the bubble phase.
                if phase.capture()
                    && event.pressed_button != Some(MouseButton::Left)
                    && TextSelection::touch_selection(window, cx).is_none()
                {
                    TextSelection::end(window, cx);
                }
            });
        },
    )
    .absolute()
    .size(px(0.))
}

impl Client {
    fn tab_bar(&self, cx: &mut Context<Self>) -> Div {
        let action_gap = px(6.);
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(34.))
            .bg(rgb(NAV))
            .text_color(rgb(NAV_TEXT))
            .child(
                div()
                    .w(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .size(px(12.))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(0x74d4dd))
                            .bg(rgb(0x245962)),
                    ),
            )
            .child(
                Self::repository_manager_button("open-repos", tr!("app.common.open"), cx)
                    .mr(action_gap),
            )
            .child(
                div().flex_1().min_w_0().h_full().child(
                    div()
                        .id("repo-tabs")
                        .flex()
                        .items_center()
                        .h_full()
                        .overflow_x_scroll()
                        .children(self.session.tabs.iter().map(|tab| {
                            let id = tab.id();
                            let active = self.session.active == Some(id);
                            div()
                                .id(("repo-tab", id))
                                .flex()
                                .items_center()
                                .h_full()
                                .w(px(156.))
                                .flex_shrink_0()
                                .gap(px(5.))
                                .pl(px(9.))
                                .pr(px(3.))
                                .border_l_1()
                                .border_color(rgb(0x50576e))
                                .bg(rgb(if active { TOOLBAR } else { NAV }))
                                .hover(|style| style.bg(rgb(TOOLBAR)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.show_tab(id, window, cx)
                                }))
                                .child(div().text_color(rgb(0x93a4c8)).child("◈"))
                                .child(div().flex_1().min_w_0().truncate().child(tab.name()))
                                .child(button(
                                    ("close-tab", id),
                                    "×",
                                    Tone::Dark,
                                    tab.can_close(),
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_tab(id, window, cx);
                                    }),
                                ))
                        }))
                        .child(
                            Self::repository_manager_button("new-tab", "+", cx)
                                .when(!self.session.tabs.is_empty(), |button| {
                                    button.ml(action_gap)
                                }),
                        ),
                ),
            )
            .child(
                button(
                    "preferences",
                    "",
                    Tone::Dark,
                    !self.session.opening,
                    cx.listener(|this, _, window, cx| this.open_preferences(window, cx)),
                )
                .w(px(31.))
                .child(icon("icons/settings.svg", NAV_TEXT))
                .when(self.preferences.is_some(), |button| {
                    button.bg(rgb(0x555f7c))
                }),
            )
            .child(
                div()
                    .px(px(12.))
                    .text_color(rgb(0xaab5d2))
                    .child("RitKraken"),
            )
    }

    fn repository_manager_button(
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        button(
            id,
            label,
            Tone::Dark,
            true,
            cx.listener(|this, _, window, cx| this.open_manager(window, cx)),
        )
        .h(px(24.))
        .flex_shrink_0()
    }

    fn toolbar(&self, tab: Option<&Tab>, cx: &mut Context<Self>) -> Div {
        let repo = tab.map_or_else(|| tr!("app.repositories.title"), Tab::name);
        let ready = tab.and_then(Tab::ready);
        let branch = ready.map_or_else(String::new, |tab| branch_label(&tab.history.data.snapshot));
        div()
            .h(px(52.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px(px(15.))
            .bg(rgb(TOOLBAR))
            .text_color(rgb(NAV_TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .min_w_0()
                    .child(toolbar_location(
                        "app.toolbar.repository",
                        repo,
                        px(200.),
                        FontWeight::SEMIBOLD,
                    ))
                    .when(!branch.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(18.))
                                .text_color(rgb(0x9da8c3))
                                .child("›"),
                        )
                        .child(toolbar_location(
                            "app.toolbar.branch_heading",
                            branch,
                            px(270.),
                            FontWeight::NORMAL,
                        ))
                    }),
            )
            .when_some(ready, |toolbar, tab| {
                toolbar.child(self.toolbar_actions(tab, cx))
            })
            .child(
                div()
                    .min_w(px(58.))
                    .text_right()
                    .text_color(rgb(0xaeb8d3))
                    .child(tr!("app.toolbar.local_git")),
            )
    }

    fn status_bar(&self, tab: Option<&Tab>) -> Div {
        let ready = tab.and_then(Tab::ready);
        let (color, message) = if self.session.opening
            || matches!(tab, Some(Tab::Saved(saved)) if saved.is_loading())
        {
            (MUTED, tr!("app.status.opening"))
        } else if let Some(tab) = ready {
            if tab.busy {
                (MUTED, tr!("app.status.running_git"))
            } else if let Some((error, message)) = &tab.notice {
                let text = message.text();
                (
                    if *error { RED } else { GREEN },
                    text.lines().next().unwrap_or(&text).to_owned(),
                )
            } else if tab.history.data.snapshot.sequence.is_some() {
                (MUTED, tr!("app.commit_menu.paused"))
            } else {
                (MUTED, tab.repo.root().to_string_lossy().into_owned())
            }
        } else if let Some(tab) = tab {
            (MUTED, tab.root().to_string_lossy().into_owned())
        } else {
            (MUTED, tr!("app.status.open_repository"))
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .h(px(24.))
            .flex_shrink_0()
            .px(px(10.))
            .bg(rgb(PAPER))
            .border_t_1()
            .border_color(rgb(LINE))
            .text_size(px(10.))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(color))
                    .child(message),
            )
            .when_some(ready, |element, tab| {
                let snapshot = &tab.history.data.snapshot;
                element.child(div().text_color(rgb(MUTED)).child(tr!(
                    "app.status.commits",
                    count = snapshot.commits.len(),
                    more = if snapshot.history_limited { "+" } else { "" }
                )))
            })
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(env!("CARGO_PKG_VERSION")),
            )
    }
}

fn toolbar_location(
    label: &'static str,
    value: String,
    max_width: Pixels,
    weight: FontWeight,
) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            heading(tr!(label))
                .text_size(px(12.))
                .text_color(rgb(0xb3bdd7)),
        )
        .child(
            div()
                .max_w(max_width)
                .truncate()
                .text_size(px(14.))
                .font_weight(weight)
                .child(value),
        )
}
