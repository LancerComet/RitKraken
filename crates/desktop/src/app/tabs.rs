use std::{cell::Cell, rc::Rc};

use gpui_kit::{prelude::*, *};

use crate::i18n::tr;

use super::{
    Client,
    theme::{Palette, palette},
    widgets::{Tone, button, icon},
};

#[derive(Clone)]
struct DraggedTab {
    id: u64,
    index: usize,
    name: SharedString,
}

struct TabDragPreview {
    name: SharedString,
}

impl Render for TabDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(28.))
            .px(px(10.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(p.accent))
            .bg(rgb(p.chrome_alt))
            .text_color(rgb(p.chrome_text))
            .shadow_md()
            .child(icon("icons/diamond.svg", p.accent).size(px(12.)))
            .child(div().max_w(px(180.)).truncate().child(self.name.clone()))
    }
}

#[derive(Clone, Default)]
pub(super) struct TabStripState {
    scroll: ScrollHandle,
    pending_reveal: Rc<Cell<Option<usize>>>,
}

impl TabStripState {
    pub(super) fn reveal(&self, index: usize) {
        self.pending_reveal.set(Some(index));
    }

    fn finish_layout(&self, window: &mut Window) {
        if let Some(index) = self.pending_reveal.take() {
            // ScrollHandle needs the viewport and overflow from its first layout.
            let scroll = self.scroll.clone();
            window.on_next_frame(move |window, _| {
                scroll.scroll_to_item(index);
                window.refresh();
            });
        }
    }
}

impl Client {
    pub(super) fn tab_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let action_gap = px(6.);
        let strip = self.session.tab_strip.clone();
        let view = cx.weak_entity();
        let accent = p.accent;
        div()
            .on_children_prepainted(move |_, window, _| strip.finish_layout(window))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(px(38.))
            .bg(rgb(p.chrome))
            .text_color(rgb(p.chrome_text))
            .border_b_1()
            .border_color(rgb(p.chrome_border))
            .child(
                div()
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .h_full()
                    .px(px(4.))
                    .child(
                        Self::repository_manager_button("open-repos", "", cx)
                            .size(px(28.))
                            .px(px(0.))
                            .rounded_full()
                            .mr(action_gap)
                            .tooltip(|window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(tr!("app.common.open"))
                                    .build(window, cx)
                            })
                            .child(icon("icons/folder-open.svg", p.chrome_text)),
                    )
                    .child(
                        div().flex_1().min_w_0().h_full().overflow_hidden().child(
                            div()
                                .id("repo-tabs")
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .w_full()
                                .h_full()
                                .overflow_x_scroll()
                                .track_scroll(&self.session.tab_strip.scroll)
                                .children(self.session.tabs.iter().enumerate().map(
                                    |(index, tab)| {
                                        let id = tab.id();
                                        let active = self.session.active == Some(id);
                                        let dragged = DraggedTab {
                                            id,
                                            index,
                                            name: tab.name().into(),
                                        };
                                        let target = view.clone();
                                        div()
                                            .id(("repo-tab", id))
                                            .flex()
                                            .items_center()
                                            .h(px(28.))
                                            .w(px(164.))
                                            .flex_shrink_0()
                                            .gap(px(6.))
                                            .pl(px(10.))
                                            .pr(px(4.))
                                            .rounded(px(10.))
                                            .border_1()
                                            .border_color(if active {
                                                rgb(p.chrome_border)
                                            } else {
                                                rgba(0x00000000)
                                            })
                                            .bg(rgb(if active { p.chrome_alt } else { p.chrome }))
                                            .hover(|style| style.bg(rgb(p.chrome_hover)))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.show_tab(id, window, cx)
                                            }))
                                            .on_drag(dragged, |tab, _, _, cx| {
                                                cx.new(|_| TabDragPreview {
                                                    name: tab.name.clone(),
                                                })
                                            })
                                            .drag_over::<DraggedTab>(move |style, dragged, _, _| {
                                                if dragged.id == id {
                                                    style
                                                } else if dragged.index < index {
                                                    style.border_r_2().border_color(rgb(accent))
                                                } else {
                                                    style.border_l_2().border_color(rgb(accent))
                                                }
                                            })
                                            .on_drop::<DraggedTab>(move |dragged, _, cx| {
                                                let _ = target.update(cx, |this, cx| {
                                                    this.move_tab_to(dragged.id, id, cx)
                                                });
                                            })
                                            .child(
                                                div()
                                                    .size(px(14.))
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        icon(
                                                            "icons/diamond.svg",
                                                            if active {
                                                                p.accent
                                                            } else {
                                                                p.chrome_muted
                                                            },
                                                        )
                                                        .size(px(12.)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .line_height(px(14.))
                                                    .truncate()
                                                    .child(tab.name()),
                                            )
                                            .child(
                                                button(
                                                    ("close-tab", id),
                                                    "",
                                                    Tone::Dark,
                                                    tab.can_close(),
                                                    cx,
                                                    cx.listener(move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.close_tab(id, window, cx);
                                                    }),
                                                )
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation();
                                                })
                                                .size(px(20.))
                                                .px(px(0.))
                                                .rounded_full()
                                                .child(
                                                    icon("icons/x.svg", p.chrome_muted)
                                                        .size(px(11.)),
                                                ),
                                            )
                                    },
                                ))
                                .child(
                                    Self::repository_manager_button("new-tab", "", cx)
                                        .w(px(26.))
                                        .px(px(0.))
                                        .rounded_full()
                                        .child(icon("icons/plus.svg", p.chrome_text).size(px(13.)))
                                        .when(!self.session.tabs.is_empty(), |button| {
                                            button.ml(px(2.))
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
                            cx,
                            cx.listener(|this, _, window, cx| this.open_preferences(window, cx)),
                        )
                        .size(px(28.))
                        .flex_shrink_0()
                        .px(px(0.))
                        .rounded_full()
                        .child(icon("icons/settings.svg", p.chrome_text))
                        .when(self.preferences.is_some(), |button| {
                            button.bg(rgb(p.chrome_hover))
                        }),
                    ),
            )
    }

    pub(super) fn move_tab_to(&mut self, dragged_id: u64, target_id: u64, cx: &mut Context<Self>) {
        let Some(from) = self
            .session
            .tabs
            .iter()
            .position(|tab| tab.id() == dragged_id)
        else {
            return;
        };
        let Some(to) = self
            .session
            .tabs
            .iter()
            .position(|tab| tab.id() == target_id)
        else {
            return;
        };
        if from == to {
            return;
        }
        let tab = self.session.tabs.remove(from);
        self.session.tabs.insert(to, tab);
        self.session.tab_strip.reveal(to);
        self.persist_session();
        cx.notify();
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
            cx,
            cx.listener(|this, _, window, cx| this.open_manager(window, cx)),
        )
        .h(px(26.))
        .flex_shrink_0()
    }
}
