#[cfg(not(target_os = "macos"))]
use gpui_kit::component::{IconName, IconNamed as _};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::{
    Client, Tab,
    message::branch_label,
    theme::Palette,
    widgets::{icon, loading::app_logo},
};

const TITLEBAR_HEIGHT: Pixels = px(36.);

impl Client {
    pub(super) fn titlebar(
        &self,
        tab: Option<&Tab>,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        platform_titlebar(tab, p, window, cx)
    }
}

fn titlebar_identity(tab: Option<&Tab>, p: Palette, cx: &App) -> Div {
    let branch_fg = if p.is_dark {
        p.accent
    } else {
        p.accent_soft_fg
    };
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .min_w_0()
        .child(img(app_logo(cx)).size(px(18.)).flex_shrink_0())
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(p.text))
                .child("RitKraken"),
        )
        .when_some(tab, |row, tab| {
            let branch = tab
                .ready()
                .map(|ready| branch_label(&ready.history.data.snapshot))
                .unwrap_or_default();
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(rgb(p.text_subtle))
                    .child("·"),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w(px(200.))
                    .truncate()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(p.text))
                    .child(tab.name()),
            )
            .when(!branch.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(rgb(p.text_subtle))
                        .child("/"),
                )
                .child(
                    div()
                        .h(px(22.))
                        .px(px(8.))
                        .min_w_0()
                        .max_w(px(220.))
                        .rounded_full()
                        .bg(rgb(p.accent_soft))
                        .border_1()
                        .border_color(rgb(p.accent_soft_border))
                        .text_color(rgb(branch_fg))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(icon("icons/git-branch.svg", branch_fg).size(px(11.)))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(branch),
                        ),
                )
            })
        })
}

#[cfg(target_os = "macos")]
struct TitleBarDragState {
    should_move: bool,
}

#[cfg(target_os = "macos")]
impl Render for TitleBarDragState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[cfg(target_os = "macos")]
fn platform_titlebar(
    tab: Option<&Tab>,
    p: Palette,
    window: &mut Window,
    cx: &mut Context<Client>,
) -> AnyElement {
    use gpui_kit::component::InteractiveElementExt as _;

    let drag_state = window.use_state(cx, |_, _| TitleBarDragState { should_move: false });
    let inset = if window.is_fullscreen() {
        px(12.)
    } else {
        px(78.)
    };
    div()
        .id("custom-titlebar")
        .w_full()
        .h(TITLEBAR_HEIGHT)
        .flex_shrink_0()
        .flex()
        .items_center()
        .bg(rgb(p.shell_bg))
        .text_color(rgb(p.text))
        .window_control_area(WindowControlArea::Drag)
        .on_double_click(|_, window, _| window.titlebar_double_click())
        .on_mouse_down_out(window.listener_for(&drag_state, |state, _, _, _| {
            state.should_move = false;
        }))
        .on_mouse_down(
            MouseButton::Left,
            window.listener_for(&drag_state, |state, _, _, _| {
                state.should_move = true;
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            window.listener_for(&drag_state, |state, _, _, _| {
                state.should_move = false;
            }),
        )
        .on_mouse_move(window.listener_for(&drag_state, |state, _, window, _| {
            if state.should_move {
                state.should_move = false;
                window.start_window_move();
            }
        }))
        .child(div().flex_1().min_w(inset).h_full())
        .child(
            div()
                .flex_shrink_0()
                .min_w_0()
                .max_w(px(560.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .child(titlebar_identity(tab, p, cx)),
        )
        .child(div().flex_1().min_w(inset).h_full())
        .into_any_element()
}

#[cfg(target_os = "linux")]
fn attach_linux_drag(drag: Stateful<Div>, window: &Window) -> Stateful<Div> {
    use gpui_kit::component::InteractiveElementExt as _;

    let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
    drag.on_double_click(|_, window, _| window.zoom_window())
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
        .when(client_decorated, |area| {
            area.on_mouse_down(MouseButton::Right, |ev, window, _| {
                window.show_window_menu(ev.position);
            })
        })
}

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
fn attach_linux_drag(drag: Stateful<Div>, _window: &Window) -> Stateful<Div> {
    drag
}

#[cfg(not(target_os = "macos"))]
fn platform_titlebar(
    tab: Option<&Tab>,
    p: Palette,
    window: &mut Window,
    cx: &mut Context<Client>,
) -> AnyElement {
    let drag = div()
        .id("titlebar-drag")
        .flex_1()
        .min_w_0()
        .h_full()
        .pl(px(12.))
        .pr(px(8.))
        .flex()
        .items_center()
        .window_control_area(WindowControlArea::Drag)
        .child(titlebar_identity(tab, p, cx))
        .map(|drag| attach_linux_drag(drag, window));

    div()
        .id("custom-titlebar")
        .w_full()
        .h(TITLEBAR_HEIGHT)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .bg(rgb(p.shell_bg))
        .text_color(rgb(p.text))
        .child(drag)
        .child(window_controls(p, window))
        .into_any_element()
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
enum WindowControl {
    Minimize,
    Maximize,
    Restore,
    Close,
}

#[cfg(not(target_os = "macos"))]
impl WindowControl {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "titlebar-minimize",
            Self::Maximize => "titlebar-maximize",
            Self::Restore => "titlebar-restore",
            Self::Close => "titlebar-close",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Maximize => IconName::WindowMaximize,
            Self::Restore => IconName::WindowRestore,
            Self::Close => IconName::WindowClose,
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn window_controls(p: Palette, window: &Window) -> Div {
    #[cfg(target_os = "linux")]
    if !matches!(window.window_decorations(), Decorations::Client { .. }) {
        return div();
    }

    let supported = window.window_controls();
    div()
        .flex()
        .items_center()
        .h_full()
        .flex_shrink_0()
        .when(supported.minimize, |strip| {
            strip.child(window_control_button(WindowControl::Minimize, p))
        })
        .when(supported.maximize, |strip| {
            let control = if window.is_maximized() {
                WindowControl::Restore
            } else {
                WindowControl::Maximize
            };
            strip.child(window_control_button(control, p))
        })
        .child(window_control_button(WindowControl::Close, p))
}

#[cfg(not(target_os = "macos"))]
fn window_control_button(control: WindowControl, p: Palette) -> Stateful<Div> {
    let is_close = matches!(control, WindowControl::Close);
    let hover_bg = if is_close { 0xe81123 } else { p.hover };
    let hover_fg = if is_close { 0xffffff } else { p.text };
    let active_bg = if is_close { 0xc50f1f } else { p.surface_high };

    let button = div()
        .id(control.id())
        .group(control.id())
        .flex()
        .items_center()
        .justify_center()
        .w(px(46.))
        .h_full()
        .flex_shrink_0()
        .hover(move |style| style.bg(rgb(hover_bg)))
        .active(move |style| style.bg(rgb(active_bg)))
        .child(
            svg()
                .path(control.icon().path())
                .size_3p5()
                .flex_shrink_0()
                .text_color(rgb(p.text_muted))
                .group_hover(control.id(), move |style| style.text_color(rgb(hover_fg))),
        );

    #[cfg(target_os = "windows")]
    return button.window_control_area(control.area());

    #[cfg(not(target_os = "windows"))]
    {
        let area = control.area();
        button
            .window_control_area(area)
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match control {
                    WindowControl::Minimize => window.minimize_window(),
                    WindowControl::Maximize | WindowControl::Restore => window.zoom_window(),
                    WindowControl::Close => window.remove_window(),
                }
            })
    }
}
