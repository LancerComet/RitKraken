use gpui_kit::{
    base::TextSelection,
    component::{Sizable as _, progress::ProgressCircle},
    prelude::*,
    *,
};

use crate::i18n::tr;

use super::{
    Client, Escape, OpenPalette, OpenPreferences, OpenRepos, Refresh, SearchCommits, Submit, Tab,
    theme::{Palette, palette},
    widgets::{activity_wave::toolbar_activity_wave, icon},
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
            .bg(rgb(p.shell_bg))
            .text_color(rgb(p.text))
            .text_size(px(12.))
            .child(finish_released_text_selection())
            .child(self.titlebar(tab, p, window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .px(px(8.))
                    .pb(px(5.))
                    .gap(px(6.))
                    .child(self.toolbar(tab, is_busy, p, window, cx))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(body),
                    )
                    .child(self.status_bar(tab, p, cx))
                    .children(modal),
            )
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
    fn toolbar(
        &self,
        tab: Option<&Tab>,
        is_busy: bool,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let wave = toolbar_activity_wave(is_busy, p, window, cx);
        let card = div()
            .relative()
            .flex_shrink_0()
            .flex()
            .items_center()
            .rounded(px(14.))
            .border_1()
            .border_color(rgb(p.chrome_border))
            .bg(rgb(p.chrome_alt))
            .text_color(rgb(p.chrome_text))
            .children(wave);
        let ready = tab.and_then(Tab::ready);
        if let Some(banner) = ready.and_then(|tab| self.push_warning_content(tab, cx)) {
            return card
                .min_h(px(54.))
                .justify_center()
                .px(px(16.))
                .py(px(8.))
                .child(banner);
        }
        card.h(px(54.))
            .gap(px(12.))
            .px(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(self.open_repo_menu(tab, p, cx)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(ready, |center, tab| {
                        center.child(self.toolbar_actions(tab, cx))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(self.toolbar_navigation(ready, cx)),
            )
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
            .h(px(22.))
            .flex_shrink_0()
            .px(px(4.))
            .text_size(px(10.5))
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
                        .bg(rgb(p.canvas))
                        .border_1()
                        .border_color(rgb(p.border))
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
                    .px(px(7.))
                    .py(px(1.))
                    .rounded_full()
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
