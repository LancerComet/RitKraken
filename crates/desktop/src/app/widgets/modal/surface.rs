use gpui_kit::{prelude::*, *};

pub(in crate::app) fn modal_layer(
    panel: AnyElement,
    surface: Option<CompositionSurfaceId>,
    progress: f32,
    closing: bool,
) -> Div {
    let opacity = if surface.is_some() { progress } else { 1.0 };
    let panel = div()
        .relative()
        .shadow_lg()
        .child(panel)
        .when(closing || opacity < 1.0, |panel| {
            // The live hitboxes are at their resting positions during the transform.
            panel.child(div().absolute().inset_0().occlude().cursor_default())
        });
    div()
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000055).opacity(opacity))
        .child(ModalSurface {
            child: panel.into_any_element(),
            surface,
        })
}

struct ModalSurface {
    child: AnyElement,
    surface: Option<CompositionSurfaceId>,
}

impl ModalSurface {
    fn draw(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        draw: impl Fn(&mut AnyElement, &mut Window, &mut App),
    ) {
        if let Some(surface) = self.surface {
            match window.with_composition_surface(surface, |window| {
                draw(&mut self.child, window, cx);
            }) {
                Ok(()) => return,
                Err(error) => eprintln!("Could not draw the modal surface: {error:#}"),
            }
        }
        draw(&mut self.child, window, cx);
    }
}

impl IntoElement for ModalSurface {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ModalSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.draw(window, cx, |child, window, cx| {
            child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.draw(window, cx, AnyElement::paint);
    }
}
