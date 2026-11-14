use gpui_kit::{
    component::{
        input::Editor,
        resizable::{h_resizable, resizable_panel, v_resizable},
    },
    prelude::*,
    *,
};
use ritkraken_diff::{DiffConflictResolution, DiffTypography};
use ritkraken_git::{ConflictResolution, has_conflict_markers};

use super::ConflictReview;
use crate::{
    app::{
        review::DiffLayout,
        theme::{Palette, palette},
        widgets::{Tone, button, icon, note},
    },
    i18n::tr,
};

impl ConflictReview {
    fn source_pane(
        &self,
        side: usize,
        typography: DiffTypography,
        wrap: bool,
        enabled: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let version = if side == 0 {
            &self.expected.current
        } else {
            &self.expected.incoming
        };
        let known = side == 0 || self.expected.unmerged;
        let content = self.sources[side].as_ref().map_or_else(
            || {
                note(
                    tr!(if !known {
                        "app.conflicts.unavailable"
                    } else if version.is_none() {
                        "app.conflicts.deleted"
                    } else {
                        "app.conflicts.binary"
                    }),
                    cx,
                )
                .into_any_element()
            },
            |state| {
                DiffLayout::Inline
                    .viewer(state, wrap, typography)
                    .into_any_element()
            },
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .h(px(36.))
                    .flex_shrink_0()
                    .bg(rgb(p.surface_high))
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(div().flex_1().min_w_0().truncate().child(tr!(if side == 0 {
                        "app.conflicts.current"
                    } else {
                        "app.conflicts.incoming"
                    })))
                    .child(
                        button(
                            ("take-conflict-file", side),
                            tr!(if version.is_none() && known {
                                "app.conflicts.take_deletion"
                            } else if side == 0 {
                                "app.conflicts.take_current"
                            } else {
                                "app.conflicts.take_incoming"
                            }),
                            Tone::Outline,
                            enabled && known,
                            cx,
                            cx.listener(move |this, _, window, cx| {
                                this.apply(
                                    if side == 0 {
                                        ConflictResolution::Current
                                    } else {
                                        ConflictResolution::Incoming
                                    },
                                    window,
                                    cx,
                                )
                            }),
                        )
                        .text_size(px(11.))
                        .px(px(8.)),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }
}

impl Render for ConflictReview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let owner = self.owner.upgrade();
        let client = owner.as_ref().map(|owner| owner.read(cx));
        let busy = client.is_none_or(|client| !client.tab(self.id).is_some_and(|tab| !tab.busy));
        let typography = client
            .map(|client| client.saved.preferences.editor.diff_typography())
            .unwrap_or_default();
        let wrap = client
            .and_then(|client| client.tab(self.id))
            .is_some_and(|tab| tab.review.wrap_lines);
        let (family, font_size, line_height) = typography.resolve(window, cx);
        let enabled = !busy && !self.stale;
        let editable = self.expected.working_text.is_some();
        let text = self.output.read(cx).value();
        let can_save = enabled && editable && !has_conflict_markers(text.as_bytes());
        let count = self.count();
        let left = self.source_pane(0, typography.clone(), wrap, enabled, p, cx);
        let right = self.source_pane(1, typography, wrap, enabled, p, cx);
        let output =
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .min_h(px(38.))
                        .py(px(5.))
                        .flex_shrink_0()
                        .px(px(10.))
                        .gap(px(6.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .bg(rgb(p.surface_high))
                        .border_b_1()
                        .border_color(rgb(p.border))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(tr!("app.conflicts.output")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .flex_shrink_0()
                                .child(tr!("app.conflicts.remaining", count = count)),
                        )
                        .children([(false, "↑"), (true, "↓")].into_iter().map(
                            |(forward, glyph)| {
                                button(
                                    ("conflict-next", usize::from(forward)),
                                    glyph,
                                    Tone::Outline,
                                    enabled && count > 0,
                                    cx,
                                    cx.listener(move |this, _, window, cx| {
                                        this.navigate(forward, window, cx)
                                    }),
                                )
                                .w(px(26.))
                                .flex_shrink_0()
                                .px(px(0.))
                            },
                        ))
                        .children(
                            [
                                (
                                    "app.conflicts.accept_current",
                                    DiffConflictResolution::Current,
                                ),
                                (
                                    "app.conflicts.accept_incoming",
                                    DiffConflictResolution::Incoming,
                                ),
                                ("app.conflicts.accept_both", DiffConflictResolution::Both),
                            ]
                            .into_iter()
                            .map(|(key, choice)| {
                                button(
                                    key,
                                    tr!(key),
                                    Tone::Outline,
                                    enabled && count > 0,
                                    cx,
                                    cx.listener(move |this, _, window, cx| {
                                        this.choose(choice, window, cx)
                                    }),
                                )
                                .px(px(8.))
                                .flex_shrink_0()
                                .text_size(px(11.))
                            }),
                        ),
                )
                .child(
                    div().flex_1().min_h_0().overflow_hidden().child(
                        Editor::new(&self.output)
                            .h_full()
                            .appearance(false)
                            .bordered(false)
                            .disabled(!enabled || !editable)
                            .font_family(family)
                            .text_size(font_size)
                            .line_height(line_height),
                    ),
                );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(p.canvas))
            .text_color(rgb(p.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(38.))
                    .px(px(10.))
                    .gap(px(8.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(
                        div().flex_1().min_w_0().truncate().child(
                            self.expected
                                .file
                                .change
                                .path
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    )
                    .child(button(
                        "reload-conflict",
                        tr!("app.conflicts.reload"),
                        Tone::Plain,
                        !busy,
                        cx,
                        cx.listener(|this, _, window, cx| this.reload(window, cx)),
                    ))
                    .child(button(
                        "save-conflict",
                        tr!(if editable {
                            "app.conflicts.save"
                        } else {
                            "app.conflicts.mark_resolved"
                        }),
                        Tone::Green,
                        if editable { can_save } else { enabled },
                        cx,
                        cx.listener(move |this, _, window, cx| {
                            let resolution = if this.expected.working_text.is_some() {
                                ConflictResolution::Text(this.output.read(cx).value().to_string())
                            } else {
                                ConflictResolution::WorkingFile
                            };
                            this.apply(resolution, window, cx);
                        }),
                    ))
                    .child(
                        button(
                            "close-conflict",
                            "",
                            Tone::Plain,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| {
                                let _ = this
                                    .owner
                                    .update(cx, |client, cx| client.close_diff(window, cx));
                            }),
                        )
                        .size(px(26.))
                        .px(px(0.))
                        .child(icon("icons/x.svg", p.text)),
                    ),
            )
            .when(self.stale, |view| {
                view.child(note(tr!("app.conflicts.stale"), cx).bg(rgb(p.warning_bg)))
            })
            .child(
                div().flex_1().min_h_0().child(
                    v_resizable("conflict-vertical")
                        .with_state(&self.vertical)
                        .child(
                            resizable_panel()
                                .size(
                                    (window.viewport_size().height * 0.32)
                                        .clamp(px(160.), px(400.)),
                                )
                                .size_range(px(100.)..px(2000.))
                                .child(
                                    h_resizable("conflict-sources")
                                        .with_state(&self.horizontal)
                                        .child(
                                            resizable_panel()
                                                .size_range(px(150.)..px(3000.))
                                                .child(left),
                                        )
                                        .child(
                                            resizable_panel()
                                                .size_range(px(150.)..px(3000.))
                                                .child(right),
                                        ),
                                ),
                        )
                        .child(
                            resizable_panel()
                                .size_range(px(120.)..px(3000.))
                                .child(output),
                        ),
                ),
            )
    }
}
