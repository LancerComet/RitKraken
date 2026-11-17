use gpui_kit::{
    base::TextSelection,
    component::{
        Sizable as _,
        progress::{Progress, ProgressCircle},
    },
    prelude::*,
    *,
};

use crate::i18n::tr;

use super::{
    Client, Escape, OpenPalette, OpenPreferences, OpenRepos, Refresh, SearchCommits, Submit, Tab,
    message::branch_label,
    theme::{Palette, palette},
    widgets::icon,
};

impl Render for Client {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let modal = self.render_modal(window, cx);
        let tab = self.session.active_tab();
        let is_busy = tab.is_some_and(Tab::is_busy) || self.session.opening;
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
                if !this.modal_is_present() {
                    this.refresh(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SearchCommits, window, cx| {
                this.open_commit_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| {
                this.open_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(p.canvas))
            .text_color(rgb(p.text))
            .text_size(px(12.))
            .child(finish_released_text_selection())
            .child(self.tab_bar(p, cx))
            .child(self.toolbar(tab, p, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(body)
                    .when(is_busy, |container| {
                        container.child(
                            div().absolute().top_0().left_0().right_0().child(
                                Progress::new("workspace-busy-progress")
                                    .loading(true)
                                    .h(px(2.5))
                                    .rounded(px(0.))
                                    .color(rgb(p.accent)),
                            ),
                        )
                    }),
            )
            .child(self.status_bar(tab, p, cx))
            .children(modal)
            .when(self.closing_modal.is_some(), |element| {
                element.capture_key_down(|event, window, cx| {
                    if event.keystroke.key == "tab" {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
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
    fn toolbar(&self, tab: Option<&Tab>, p: Palette, cx: &mut Context<Self>) -> Div {
        let repo = tab.map_or_else(|| tr!("app.repositories.title"), Tab::name);
        let ready = tab.and_then(Tab::ready);
        if let Some(banner) = ready.and_then(|tab| self.push_warning(tab, cx)) {
            return banner;
        }
        let branch = ready.map_or_else(String::new, |tab| branch_label(&tab.history.data.snapshot));
        div()
            .h(px(54.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.))
            .bg(rgb(p.chrome_alt))
            .text_color(rgb(p.chrome_text))
            .border_b_1()
            .border_color(rgb(p.border))
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
                        p,
                    ))
                    .when(!branch.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(18.))
                                .text_color(rgb(p.chrome_muted))
                                .child("›"),
                        )
                        .child(toolbar_location(
                            "app.toolbar.branch_heading",
                            branch,
                            px(270.),
                            FontWeight::NORMAL,
                            p,
                        ))
                    }),
            )
            .when_some(ready, |toolbar, tab| {
                toolbar.child(self.toolbar_actions(tab, cx))
            })
            .child(self.toolbar_navigation(ready, cx))
    }

    fn status_bar(&self, tab: Option<&Tab>, p: Palette, cx: &mut Context<Self>) -> Div {
        let ready = tab.and_then(Tab::ready);
        let opening =
            self.session.opening || matches!(tab, Some(Tab::Saved(saved)) if saved.is_loading());
        let active_spinner = opening || ready.is_some_and(|tab| tab.busy);
        let (color, message) = if opening {
            (p.text_muted, tr!("app.status.opening"))
        } else if let Some(tab) = ready {
            if tab.busy {
                (p.text_muted, tr!("app.status.running_git"))
            } else if tab.history.data.snapshot.sequence.is_some() {
                (p.text_muted, tr!("app.commit_menu.paused"))
            } else {
                (p.text_muted, tab.repo.root().to_string_lossy().into_owned())
            }
        } else if let Some(tab) = tab {
            (p.text_muted, tab.root().to_string_lossy().into_owned())
        } else {
            (p.text_muted, tr!("app.status.open_repository"))
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .h(px(26.))
            .flex_shrink_0()
            .px(px(12.))
            .bg(rgb(p.surface_dim))
            .border_t_1()
            .border_color(rgb(p.border))
            .text_size(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .when(active_spinner, |row| {
                        row.child(
                            ProgressCircle::new("status-busy-spinner")
                                .loading(true)
                                .small()
                                .color(rgb(p.accent)),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_color(rgb(color))
                            .child(message),
                    ),
            )
            .when_some(ready, |element, tab| {
                let snapshot = &tab.history.data.snapshot;
                element.child(
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(rgb(p.surface_high))
                        .text_color(rgb(p.text_muted))
                        .child(tr!(
                            "app.status.commits",
                            count = snapshot.commits.len(),
                            more = if snapshot.history_limited { "+" } else { "" }
                        )),
                )
            })
            .child(
                div()
                    .id("open-logs")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .py(px(1.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .text_color(rgb(p.text_muted))
                    .hover(|button| button.bg(rgb(p.hover)).text_color(rgb(p.text)))
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tr!("app.logs.title"))
                            .build(window, cx)
                    })
                    .child(icon("icons/scroll-text.svg", p.text_muted).size(px(12.)))
                    .child(tr!("app.logs.title"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_logs(window, cx))),
            )
            .child(
                div()
                    .text_color(rgb(p.text_muted))
                    .child(env!("CARGO_PKG_VERSION")),
            )
    }
}

fn toolbar_location(
    label: &'static str,
    value: String,
    max_width: Pixels,
    weight: FontWeight,
    p: Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(p.chrome_muted))
                .child(tr!(label)),
        )
        .child(
            div()
                .max_w(max_width)
                .truncate()
                .text_size(px(14.))
                .font_weight(weight)
                .text_color(rgb(p.chrome_text))
                .child(value),
        )
}
