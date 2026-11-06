use gpui_kit::{prelude::*, *};

use super::{
    Client, Escape, OpenPreferences, OpenRepos, Refresh, RepoTab, Submit,
    widgets::{
        GREEN, LINE, MUTED, NAV, NAV_TEXT, PAPER, RED, TEXT, TOOLBAR, Tone, WHITE, button, heading,
        icon,
    },
};

impl Render for Client {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.current();
        let body = if let Some(preferences) = &self.preferences {
            self.preferences_view(preferences, cx).into_any_element()
        } else if let Some(tab) = tab {
            self.workspace(tab, window, cx).into_any_element()
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
            .child(self.tab_bar(cx))
            .when(self.preferences.is_none(), |element| {
                element.child(self.toolbar(tab, cx))
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(body),
            )
            .when(self.preferences.is_none(), |element| {
                element.child(self.status_bar(tab))
            })
            .when_some(self.manager.mode, |element, mode| {
                element.child(self.manager_view(mode, window, cx))
            })
            .when(self.branch_dialog.open, |element| {
                element.child(self.branch_view(cx))
            })
    }
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
            .child(Self::repository_manager_button("open-repos", "Open", cx).mr(action_gap))
            .child(
                div().flex_1().min_w_0().h_full().child(
                    div()
                        .id("repo-tabs")
                        .flex()
                        .items_center()
                        .h_full()
                        .overflow_x_scroll()
                        .children(self.session.tabs.iter().map(|tab| {
                            let id = tab.id;
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
                                .child(div().flex_1().min_w_0().truncate().child(tab.repo.name()))
                                .child(button(
                                    ("close-tab", id),
                                    "×",
                                    Tone::Dark,
                                    !tab.busy,
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
        label: &'static str,
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

    fn toolbar(&self, tab: Option<&RepoTab>, cx: &mut Context<Self>) -> Div {
        let (repo, branch) = tab.map_or_else(
            || ("Repositories".to_owned(), String::new()),
            |tab| (tab.repo.name(), tab.history.data.snapshot.branch.clone()),
        );
        div()
            .h(px(48.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.))
            .bg(rgb(TOOLBAR))
            .text_color(rgb(NAV_TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(heading("REPOSITORY").text_color(rgb(0xb3bdd7)))
                            .child(
                                div()
                                    .max_w(px(200.))
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(repo),
                            ),
                    )
                    .when(!branch.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(18.))
                                .text_color(rgb(0x9da8c3))
                                .child("›"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(heading("BRANCH").text_color(rgb(0xb3bdd7)))
                                .child(div().max_w(px(270.)).truncate().child(branch)),
                        )
                    }),
            )
            .when_some(tab, |toolbar, tab| {
                toolbar.child(self.toolbar_actions(tab, cx))
            })
            .child(
                div()
                    .min_w(px(58.))
                    .text_right()
                    .text_color(rgb(0xaeb8d3))
                    .child("Local Git"),
            )
    }

    fn status_bar(&self, tab: Option<&RepoTab>) -> Div {
        let (color, message) = if self.session.opening {
            (MUTED, "Opening repository…".to_owned())
        } else if let Some(tab) = tab {
            if tab.busy {
                (MUTED, "Running Git…".to_owned())
            } else if let Some((error, message)) = &tab.notice {
                (
                    if *error { RED } else { GREEN },
                    message.lines().next().unwrap_or(message).to_owned(),
                )
            } else {
                (MUTED, tab.repo.root().to_string_lossy().into_owned())
            }
        } else {
            (MUTED, "Open a repository to begin".to_owned())
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
            .when_some(tab, |element, tab| {
                let snapshot = &tab.history.data.snapshot;
                element.child(div().text_color(rgb(MUTED)).child(format!(
                    "{}{} commits",
                    snapshot.commits.len(),
                    if snapshot.history_limited { "+" } else { "" }
                )))
            })
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(env!("CARGO_PKG_VERSION")),
            )
    }
}
