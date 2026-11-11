use std::{any::Any, cell::Cell, rc::Rc};

use anyhow::{Context as _, Result};
use gpui_kit::{
    Bounds, DevicePixels, PlatformSurfaceAttachment, Point, WindowComposition,
    WindowCompositionSurface,
};
use windows::{
    Win32::Graphics::DirectComposition::{
        DCOMPOSITION_OPACITY_MODE_LAYER, DCompositionCreateDevice3, IDCompositionDesktopDevice,
        IDCompositionEffectGroup, IDCompositionVisual2,
    },
    core::{IUnknown, Interface},
};
use windows_numerics::Matrix3x2;

pub(super) struct Container {
    pub(super) surface: WindowCompositionSurface,
    group: Rc<VisualGroup>,
}

impl Container {
    pub(super) fn new(composition: &WindowComposition<'_>) -> Result<Self> {
        let group = Rc::new(VisualGroup::new()?);
        let surface = composition.create_external_gpu_surface(group.clone())?;
        Ok(Self { surface, group })
    }

    pub(super) fn present(
        &self,
        viewport: Bounds<DevicePixels>,
        scale: f32,
        opacity: f32,
    ) -> Result<()> {
        let offset = (1.0 - scale) * 0.5;
        let transform = Matrix3x2 {
            M11: scale,
            M22: scale,
            M31: viewport.size.width.0 as f32 * offset,
            M32: viewport.size.height.0 as f32 * offset,
            ..Default::default()
        };
        // The external visual has its own device. Its children are rendered by GPUI.
        unsafe {
            self.group.visual.SetTransform2(&transform)?;
            self.group.effect.SetOpacity2(opacity)?;
            self.group.device.Commit()?;
        }
        Ok(())
    }
}

struct VisualGroup {
    device: IDCompositionDesktopDevice,
    visual: IDCompositionVisual2,
    effect: IDCompositionEffectGroup,
    bounds: Cell<Bounds<DevicePixels>>,
    parent_origin: Cell<Point<DevicePixels>>,
}

impl VisualGroup {
    fn new() -> Result<Self> {
        // This device owns only a composition visual and effect, with no swap chain.
        let device: IDCompositionDesktopDevice =
            unsafe { DCompositionCreateDevice3(None::<&IUnknown>) }
                .context("create the modal DirectComposition device")?;
        let visual = unsafe { device.CreateVisual()? };
        let effect = unsafe { device.CreateEffectGroup()? };
        unsafe {
            visual.SetOpacityMode(DCOMPOSITION_OPACITY_MODE_LAYER)?;
            visual.SetEffect(&effect)?;
        }
        Ok(Self {
            device,
            visual,
            effect,
            bounds: Cell::default(),
            parent_origin: Cell::default(),
        })
    }

    fn position(&self, origin: Point<DevicePixels>, parent: Point<DevicePixels>) -> Result<()> {
        unsafe {
            self.visual.SetOffsetX2((origin.x - parent.x).0 as f32)?;
            self.visual.SetOffsetY2((origin.y - parent.y).0 as f32)?;
        }
        Ok(())
    }
}

impl PlatformSurfaceAttachment for VisualGroup {
    fn set_bounds(&self, bounds: Bounds<DevicePixels>) -> Result<()> {
        self.position(bounds.origin, self.parent_origin.get())?;
        self.bounds.set(bounds);
        Ok(())
    }

    fn bounds(&self) -> Bounds<DevicePixels> {
        self.bounds.get()
    }

    fn set_parent_origin(&self, origin: Point<DevicePixels>) -> Result<()> {
        // GPUI calls this before reattaching the GPUI visual under the group.
        unsafe { self.visual.RemoveAllVisuals()? };
        self.position(self.bounds.get().origin, origin)?;
        self.parent_origin.set(origin);
        Ok(())
    }

    fn set_visible(&self, visible: bool) -> Result<()> {
        unsafe {
            self.effect.SetOpacity2(if visible { 1.0 } else { 0.0 })?;
            self.device.Commit()?;
        }
        Ok(())
    }

    fn platform_handle(&self) -> Result<Box<dyn Any>> {
        Ok(Box::new(self.visual.cast::<IUnknown>()?))
    }
}
