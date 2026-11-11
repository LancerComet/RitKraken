use std::cell::Cell;

use anyhow::{Context as _, Result, anyhow};
use gpui_kit::{Bounds, DevicePixels, WindowComposition, WindowCompositionSurface};
use objc2_app_kit::NSView;
use objc2_core_foundation::CGAffineTransform;
use objc2_quartz_core::CATransaction;

pub(super) struct Container {
    pub(super) surface: WindowCompositionSurface,
    presentation: Cell<Option<(Bounds<DevicePixels>, f32)>>,
}

impl Container {
    pub(super) fn new(composition: &WindowComposition<'_>) -> Result<Self> {
        Ok(Self {
            surface: composition.create_native_surface()?,
            presentation: Cell::default(),
        })
    }

    pub(super) fn present(
        &self,
        bounds: Bounds<DevicePixels>,
        scale: f32,
        opacity: f32,
    ) -> Result<()> {
        if self.presentation.get() == Some((bounds, opacity)) {
            return Ok(());
        }
        let attachment = self.surface.platform_surface()?;
        let handle = attachment
            .platform_handle()?
            .downcast::<usize>()
            .map_err(|_| anyhow!("expected an AppKit view"))?;
        // GPUI owns this layer-backed NSView and calls the presenter on its UI thread.
        let view = unsafe { (*handle as *const NSView).as_ref() }.context("missing modal view")?;
        let layer = view.layer().context("missing modal composition layer")?;
        let scale = f64::from(scale);
        let anchor = layer.anchorPoint();
        let dimensions = layer.bounds().size;
        let transform = CGAffineTransform {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: scale,
            tx: (0.5 - anchor.x) * dimensions.width * (1.0 - scale),
            ty: (0.5 - anchor.y) * dimensions.height * (1.0 - scale),
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        layer.setAllowsGroupOpacity(true);
        layer.setAffineTransform(transform);
        layer.setOpacity(opacity);
        CATransaction::commit();
        self.presentation.set(Some((bounds, opacity)));
        Ok(())
    }
}
