mod composition;
mod surface;

use gpui_kit::{base::FocusTrapElement as _, component::tooltip::Tooltip, prelude::*, *};

use super::{HEADER, LINE, Tone, WHITE, button};
use crate::i18n::tr;

pub(in crate::app) use composition::ModalCompositor;
pub(in crate::app) use surface::modal_layer;
pub(in crate::app) const MODAL_CORNER_RADIUS: Pixels = px(4.);

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(gpui_kit::IntoElement)]
pub(in crate::app) struct Modal {
    id: &'static str,
    title: SharedString,
    focus: FocusHandle,
    close_enabled: bool,
    on_close: CloseHandler,
    children: Vec<AnyElement>,
    viewport_fractions: Option<Size<f32>>,
    maximum_size: Size<Pixels>,
}

impl Modal {
    pub fn new(
        id: &'static str,
        title: impl Into<SharedString>,
        focus: &FocusHandle,
        close_enabled: bool,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            focus: focus.clone(),
            close_enabled,
            on_close: Box::new(on_close),
            children: Vec::new(),
            viewport_fractions: None,
            maximum_size: size(px(900.), px(650.)),
        }
    }

    pub fn relative_size(mut self, fractions: Size<f32>, maximum: Size<Pixels>) -> Self {
        self.viewport_fractions = Some(fractions);
        self.maximum_size = maximum;
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let dimensions = window.viewport_size();
        let available = match self.viewport_fractions {
            Some(fractions) => size(
                dimensions.width * fractions.width,
                dimensions.height * fractions.height,
            ),
            None => size(dimensions.width - px(44.), dimensions.height - px(62.)),
        };
        let close_label = tr!("app.common.close");
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(45.))
            .flex_shrink_0()
            .px(px(13.))
            .py(px(3.))
            .bg(rgb(HEADER))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title.clone()),
            )
            .child(
                button(
                    "close-modal",
                    "×",
                    Tone::Plain,
                    self.close_enabled,
                    self.on_close,
                )
                .text_size(px(21.6))
                .aria_label(close_label.clone())
                .tooltip(move |window, cx| Tooltip::new(close_label.clone()).build(window, cx)),
            );
        div()
            .id(self.id)
            .key_context("Modal")
            .role(Role::Dialog)
            .aria_label(self.title)
            .w(available.width.min(self.maximum_size.width))
            .h(available.height.min(self.maximum_size.height))
            .flex()
            .flex_col()
            .bg(rgb(WHITE))
            .rounded(MODAL_CORNER_RADIUS)
            .border_1()
            .border_color(rgb(0xaeb4bf))
            .child(header)
            .children(self.children)
            .focus_trap(self.id, &self.focus)
    }
}
