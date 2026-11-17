use gpui_kit::{
    component::{
        input::{self, Input},
        kbd::Kbd,
    },
    prelude::*,
    *,
};

use super::PaletteForm;
use crate::{
    app::{
        Escape, OpenPalette, Submit,
        theme::palette,
        widgets::{Tone, button, error_text},
    },
    i18n::tr,
};

impl Render for PaletteForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .id("command-palette")
            .key_context("CommandPalette")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            // Input's own handlers consume these actions. Capture them before
            // they reach the field, rather than relying on root graph bindings.
            .capture_action(cx.listener(|this, _: &input::MoveUp, _, cx| {
                cx.stop_propagation();
                this.move_selection(-1, cx);
            }))
            .capture_action(cx.listener(|this, _: &input::MoveDown, _, cx| {
                cx.stop_propagation();
                this.move_selection(1, cx);
            }))
            .capture_action(cx.listener(|this, _: &input::Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.modal.close(window, cx);
            }))
            .on_action(cx.listener(|_, _: &Submit, _, cx| cx.stop_propagation()))
            .child(Input::new(&self.input).w_full())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(tr!("app.palette.hint")),
                    )
                    .children(Kbd::binding_for_action(
                        &OpenPalette,
                        Some("RitKraken"),
                        window,
                    )),
            )
            .when(self.pending > 0, |body| {
                body.child(
                    div()
                        .text_color(rgb(p.text_muted))
                        .child(tr!("app.palette.loading")),
                )
            })
            .children(self.error.as_ref().map(|error| error_text(error, cx)))
            .when(self.filtered.is_empty(), |body| {
                body.child(
                    div()
                        .text_color(rgb(p.text_muted))
                        .child(tr!("app.palette.empty")),
                )
            })
            .child(
                uniform_list(
                    "palette-results",
                    self.filtered.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let p = palette(cx);
                        range
                            .map(|index| {
                                let candidate = &this.candidates[this.filtered[index]];
                                div()
                                    .id(("palette-result", index))
                                    .w_full()
                                    .h(px(54.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .border_b_1()
                                    .border_color(rgb(p.border))
                                    .when(index == this.selected, |row| row.bg(rgb(p.hover)))
                                    .hover(|style| style.bg(rgb(p.hover)))
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .w(px(78.))
                                            .flex_shrink_0()
                                            .text_size(px(11.))
                                            .text_color(rgb(p.text_muted))
                                            .child(candidate.kind().label()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .h_full()
                                            .flex()
                                            .flex_col()
                                            .justify_center()
                                            .child(div().truncate().child(candidate.title.clone()))
                                            .when(!candidate.details.is_empty(), |column| {
                                                column.child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(11.))
                                                        .text_color(rgb(p.text_muted))
                                                        .child(candidate.details.clone()),
                                                )
                                            }),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.selected = index;
                                        this.execute(window, cx);
                                    }))
                            })
                            .collect()
                    }),
                )
                .w_full()
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0(),
            )
            .child(div().flex().justify_end().child(button(
                "palette-execute",
                tr!("app.palette.execute"),
                Tone::Primary,
                !self.filtered.is_empty(),
                cx,
                cx.listener(|this, _, window, cx| this.execute(window, cx)),
            )))
    }
}
