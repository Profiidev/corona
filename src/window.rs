//! [`slint::platform::WindowAdapter`] backed by femtovg's wgpu renderer + our own wgpu::Surface.
//! One per layer surface. Unlike the OpenGL renderer, `FemtoVGWGPURenderer` doesn't present on
//! its own — we own the acquire/render/present sequence here (see `render_if_dirty`).

use std::cell::Cell;
use std::rc::{Rc, Weak};

use slint::platform::femtovg_renderer::FemtoVGWGPURenderer;
use slint::platform::{Renderer, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, Window, WindowSize};

use crate::gpu::GpuContext;

pub struct CoronaWindow {
    window: Window,
    renderer: FemtoVGWGPURenderer,
    surface: wgpu::Surface<'static>,
    gpu: Rc<GpuContext>,
    dirty: Cell<bool>,
    size: Cell<PhysicalSize>,
}

impl CoronaWindow {
    pub fn new(gpu: Rc<GpuContext>, surface: wgpu::Surface<'static>, initial_size: PhysicalSize) -> Result<Rc<Self>, slint::PlatformError> {
        let renderer = FemtoVGWGPURenderer::new(gpu.instance.clone(), gpu.device.clone(), gpu.queue.clone())?;
        Ok(Rc::new_cyclic(|weak_self| {
            let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);
            Self { window, renderer, surface, gpu, dirty: Cell::new(false), size: Cell::new(initial_size) }
        }))
    }

    /// Called after the compositor's `configure` gives us a real size — reconfigures the wgpu
    /// surface (femtovg's GL renderer used to do this resize step for us automatically; here we
    /// drive presentation ourselves, so we drive the resize too) and marks the window dirty.
    pub fn set_physical_size(&self, size: PhysicalSize) {
        self.size.set(size);
        if let Err(e) = self.gpu.configure_surface(&self.surface, size.width, size.height) {
            tracing::warn!("failed to reconfigure wgpu surface: {e:#}");
        }
        self.window.dispatch_event(WindowEvent::Resized { size: size.to_logical(self.window.scale_factor()) });
        self.dirty.set(true);
    }

    /// Render if something changed since the last call. No-op otherwise (nothing to redraw).
    pub fn render_if_dirty(&self) -> Result<(), slint::PlatformError> {
        if !self.dirty.replace(false) {
            return Ok(());
        }

        let texture = match self.acquire_frame() {
            Some(texture) => texture,
            // Nothing to draw this tick (occluded/timeout/lost) — try again once something marks
            // the window dirty again (a resize, a re-render request, ...).
            None => {
                self.dirty.set(true);
                return Ok(());
            }
        };

        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.render_to_texture_view(&view, texture.texture.width(), texture.texture.height(), texture.texture.format())?;
        texture.present();
        Ok(())
    }

    /// Acquires the current swapchain texture, transparently reconfiguring+retrying once on
    /// `Outdated`/`Lost` (e.g. after a compositor-driven resize we didn't initiate).
    fn acquire_frame(&self) -> Option<wgpu::SurfaceTexture> {
        use wgpu::CurrentSurfaceTexture;

        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) => Some(t),
            CurrentSurfaceTexture::Suboptimal(t) => {
                let size = self.size.get();
                if let Err(e) = self.gpu.configure_surface(&self.surface, size.width, size.height) {
                    tracing::warn!("failed to reconfigure suboptimal wgpu surface: {e:#}");
                }
                Some(t)
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                let size = self.size.get();
                if let Err(e) = self.gpu.configure_surface(&self.surface, size.width, size.height) {
                    tracing::warn!("failed to reconfigure outdated wgpu surface: {e:#}");
                    return None;
                }
                match self.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => Some(t),
                    _ => None,
                }
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded | CurrentSurfaceTexture::Validation => None,
        }
    }
}

impl WindowAdapter for CoronaWindow {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn set_size(&self, _size: WindowSize) {
        // The compositor (not the app) owns our size via layer-shell configure; ignore requests
        // from Slint itself (e.g. layout-driven resize) — set_physical_size is the real path in.
    }

    fn request_redraw(&self) {
        self.dirty.set(true);
    }
}
