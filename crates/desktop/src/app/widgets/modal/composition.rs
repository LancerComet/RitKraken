use anyhow::Result;
use gpui_kit::{base::animation::Lerp, *};

#[cfg(target_os = "macos")]
#[path = "composition/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "composition/windows.rs"]
mod platform;

const MIN_MODAL_SCALE: f32 = 0.85;

#[derive(Default)]
pub(in crate::app) struct ModalCompositor {
    panel: Option<CompositedPanel>,
    unavailable: bool,
}

impl ModalCompositor {
    pub fn finish_entering(&mut self, window: &Window) {
        if let Some(panel) = self.panel.take() {
            // Present the fully open panel in the window before retiring its animation surface.
            window.on_next_frame(move |window, _| {
                panel.remove(window);
                window.refresh();
            });
        }
    }

    pub fn update(&mut self, window: &Window, opacity: f32) -> Option<CompositionSurfaceId> {
        if self.unavailable {
            return None;
        }
        match self.update_surface(window, opacity) {
            Ok(surface) => Some(surface),
            Err(error) => {
                self.clear(window);
                self.unavailable = true;
                eprintln!("Could not composite the modal: {error:#}");
                None
            }
        }
    }

    fn update_surface(&mut self, window: &Window, opacity: f32) -> Result<CompositionSurfaceId> {
        if self.panel.is_none() {
            self.panel = Some(CompositedPanel::new(window)?);
        }
        let panel = self.panel.as_mut().expect("modal surface was initialized");
        let viewport = window
            .viewport_size()
            .to_device_pixels(window.scale_factor());
        let bounds = Bounds::new(Point::default(), viewport);
        // GPUI clips the panel's hitboxes to the bounds of its native parent.
        panel
            .container
            .surface
            .platform_surface()?
            .set_bounds(bounds)?;
        let scale = MIN_MODAL_SCALE.lerp(&1.0, opacity);
        panel.container.present(bounds, scale, opacity)?;
        Ok(panel.content)
    }

    pub fn clear(&mut self, window: &Window) {
        if let Some(panel) = self.panel.take() {
            panel.remove(window);
        }
        self.unavailable = false;
    }
}

struct CompositedPanel {
    container: platform::Container,
    content: CompositionSurfaceId,
}

impl CompositedPanel {
    fn new(window: &Window) -> Result<Self> {
        let composition = window.enable_window_composition()?;
        let container = platform::Container::new(&composition)?;
        let content = match composition.create_gpui_surface(Some(container.surface.id())) {
            Ok(content) => content.id(),
            Err(error) => {
                composition.remove_surface(container.surface.id())?;
                return Err(error);
            }
        };
        Ok(Self { container, content })
    }

    fn remove(self, window: &Window) {
        let hide = self
            .container
            .surface
            .platform_surface()
            .and_then(|attachment| attachment.set_visible(false));
        let remove = window.enable_window_composition().and_then(|composition| {
            let remove_content = composition.remove_surface(self.content);
            let remove_container = composition.remove_surface(self.container.surface.id());
            remove_content.and(remove_container)
        });
        if let Err(error) = remove.and(hide) {
            eprintln!("Could not remove the modal surface: {error:#}");
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use super::*;

    pub(super) struct Container {
        pub(super) surface: WindowCompositionSurface,
    }

    impl Container {
        pub(super) fn new(_: &WindowComposition<'_>) -> Result<Self> {
            anyhow::bail!("native modal composition is unavailable on this platform")
        }

        pub(super) fn present(&self, _: Bounds<DevicePixels>, _: f32, _: f32) -> Result<()> {
            anyhow::bail!("native modal composition is unavailable on this platform")
        }
    }
}
