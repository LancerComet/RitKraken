use gpui_kit::{
    component::{
        Sizable, button::Button, input::Input, popover::Popover, scroll::ScrollableElement,
    },
    prelude::*,
    *,
};
use ritkraken_git::{RefKind, Reference};

use crate::{app::theme::palette, i18n::tr};

use super::{CompareRef, PickerSide, RefCompare};

impl RefCompare {
    /// A `Button` that opens a filterable HEAD/local/remote/tag picker and
    /// replaces that side of the comparison on selection.
    pub(super) fn ref_picker(&self, side: PickerSide, cx: &mut Context<Self>) -> Popover {
        let p = palette(cx);
        let (id, current) = match side {
            PickerSide::Base => (0usize, &self.base),
            PickerSide::Compare => (1, &self.compare),
        };
        let owner = cx.weak_entity();
        let filter = self.filter.clone();
        let head = CompareRef::head(&self.snapshot);
        let refs = self.snapshot.refs.clone();
        let selected = current.revision.clone();
        Popover::new(("compare-ref", id))
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .p(px(0.))
            .w(px(280.))
            .max_h(px(360.))
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.border))
            .trigger(
                Button::new(("compare-ref-trigger", id))
                    .outline()
                    .small()
                    .min_w_0()
                    .dropdown_caret(true)
                    .label(current.label.clone()),
            )
            .content(move |_, _, popover_cx| {
                let p = palette(popover_cx);
                let text = filter.read(popover_cx).value().to_lowercase();
                let matches = |name: &str| text.is_empty() || name.to_lowercase().contains(&text);
                let mut list = div().flex().flex_col().pb(px(4.));
                list = list.child(ref_row(
                    head.label.clone(),
                    head.revision == selected,
                    p,
                    popover_cx.listener({
                        let owner = owner.clone();
                        let head = head.clone();
                        move |state, _, window, cx| {
                            state.dismiss(window, cx);
                            let head = head.clone();
                            let _ = owner.update(cx, |this, cx| {
                                this.set_ref(side, head, window, cx);
                            });
                        }
                    }),
                ));
                for (kind, label_key) in [
                    (RefKind::Local, "app.refs.local_branches"),
                    (RefKind::Remote, "app.refs.remote_branches"),
                    (RefKind::Tag, "app.refs.tags"),
                ] {
                    let group: Vec<&Reference> = refs
                        .iter()
                        .filter(|reference| reference.kind == kind && matches(&reference.name))
                        .collect();
                    if group.is_empty() {
                        continue;
                    }
                    list = list.child(
                        div()
                            .px(px(10.))
                            .pt(px(8.))
                            .pb(px(2.))
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(p.text_muted))
                            .child(tr!(label_key)),
                    );
                    for reference in group {
                        let selected_ref = CompareRef::reference(reference);
                        list = list.child(ref_row(
                            selected_ref.label.clone(),
                            selected_ref.revision == selected,
                            p,
                            popover_cx.listener({
                                let owner = owner.clone();
                                move |state, _, window, cx| {
                                    state.dismiss(window, cx);
                                    let selected_ref = selected_ref.clone();
                                    let _ = owner.update(cx, |this, cx| {
                                        this.set_ref(side, selected_ref, window, cx);
                                    });
                                }
                            }),
                        ));
                    }
                }
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(6.))
                            .flex_shrink_0()
                            .child(Input::new(&filter).small()),
                    )
                    .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(list))
            })
    }
}

fn ref_row(
    label: String,
    selected: bool,
    p: crate::app::theme::Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(format!("compare-pick-{label}"))
        .flex()
        .items_center()
        .px(px(10.))
        .py(px(5.))
        .cursor_pointer()
        .when(selected, |d| d.bg(rgb(p.border)))
        .hover(|d| d.bg(rgb(p.hover)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.))
                .text_color(rgb(p.text))
                .child(label),
        )
        .on_click(on_click)
}
