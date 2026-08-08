use std::{
  cell::{Cell, RefCell},
  rc::{Rc, Weak},
};

use slint::{
  PhysicalSize, PlatformError, Window, WindowSize,
  platform::{
    Platform, Renderer, WindowAdapter, WindowEvent, femtovg_renderer::FemtoVGWGPURenderer,
  },
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::Proxy;
use wgpu::CurrentSurfaceTexture;

use crate::adapter::gpu::GpuContext;

pub struct SlintCustomPlatform {
  pending: RefCell<Option<Rc<SlintWindow>>>,
  gpu: Rc<GpuContext>,
}

struct SlintCustomPlatformPointer(Rc<SlintCustomPlatform>);

#[derive(Debug, thiserror::Error)]
pub enum SlintCustomPlatformError {
  #[error("slint::platform::set_platform() already called")]
  PlatformAlreadySet,
  #[error("GPU error: {0}")]
  Gpu(#[from] crate::adapter::gpu::GpuError),
  #[error("failed to create FemtoVGWGPURenderer: {0}")]
  FemtoVGWGPURenderer(#[from] PlatformError),
  #[error(
    "a window is already pending creation. You need to create the Slint component for the previous window before creating a new one."
  )]
  WindowAlreadyPending,
}

impl SlintCustomPlatform {
  pub fn init(gpu: Rc<GpuContext>) -> Result<Rc<Self>, SlintCustomPlatformError> {
    let platform = Rc::new(Self {
      pending: RefCell::new(None),
      gpu,
    });

    slint::platform::set_platform(Box::new(SlintCustomPlatformPointer(platform.clone())))
      .map_err(|_| SlintCustomPlatformError::PlatformAlreadySet)?;

    Ok(platform)
  }

  pub fn create_window(
    &self,
    layer_surface: LayerSurface,
    width: u32,
    height: u32,
  ) -> Result<Rc<SlintWindow>, SlintCustomPlatformError> {
    if self.pending.borrow().as_ref().is_some() {
      return Err(SlintCustomPlatformError::WindowAlreadyPending);
    }

    let wgpu_surface = self
      .gpu
      .create_surface(&layer_surface.wl_surface().id(), width, height)?;
    let window = SlintWindow::new(
      self.gpu.clone(),
      layer_surface,
      wgpu_surface,
      PhysicalSize::new(width, height),
    )?;

    self.pending.borrow_mut().replace(window.clone());
    Ok(window)
  }
}

impl Platform for SlintCustomPlatform {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    self
      .pending
      .borrow_mut()
      .take()
      .map(|w| w as Rc<dyn WindowAdapter>)
      .ok_or(PlatformError::NoPlatform)
  }
}

impl Platform for SlintCustomPlatformPointer {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    self.0.create_window_adapter()
  }
}

pub struct SlintWindow {
  window: Window,
  renderer: FemtoVGWGPURenderer,
  surface: wgpu::Surface<'static>,
  gpu: Rc<GpuContext>,
  dirty: Cell<bool>,
  size: Cell<PhysicalSize>,
  #[allow(dead_code)]
  layer_surface: LayerSurface,
}

impl SlintWindow {
  fn new(
    gpu: Rc<GpuContext>,
    layer_surface: LayerSurface,
    surface: wgpu::Surface<'static>,
    initial_size: PhysicalSize,
  ) -> Result<Rc<Self>, SlintCustomPlatformError> {
    let renderer =
      FemtoVGWGPURenderer::new(gpu.instance.clone(), gpu.device.clone(), gpu.queue.clone())?;

    Ok(Rc::new_cyclic(|weak_self| {
      let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);

      Self {
        window,
        renderer,
        surface,
        gpu,
        dirty: Cell::new(false),
        size: Cell::new(initial_size),
        layer_surface,
      }
    }))
  }

  pub fn set_physical_size(&self, size: PhysicalSize) {
    self.size.set(size);
    if let Err(e) = self
      .gpu
      .configure_surface(&self.surface, size.width, size.height)
    {
      tracing::warn!("failed to reconfigure wgpu surface: {e:#}");
    }
    self.window.dispatch_event(WindowEvent::Resized {
      size: size.to_logical(self.window.scale_factor()),
    });
    self.dirty.set(true);
  }

  pub fn render_if_dirty(&self) -> Result<(), SlintCustomPlatformError> {
    if !self.dirty.replace(false) {
      return Ok(());
    }

    let texture = match self.acquire_frame() {
      Some(texture) => texture,
      None => {
        self.dirty.set(true);
        return Ok(());
      }
    };

    let view = texture
      .texture
      .create_view(&wgpu::TextureViewDescriptor::default());
    self.renderer.render_to_texture_view(
      &view,
      texture.texture.width(),
      texture.texture.height(),
      texture.texture.format(),
    )?;
    texture.present();

    Ok(())
  }

  fn acquire_frame(&self) -> Option<wgpu::SurfaceTexture> {
    match self.surface.get_current_texture() {
      CurrentSurfaceTexture::Success(t) => Some(t),
      CurrentSurfaceTexture::Suboptimal(t) => {
        let size = self.size.get();
        if let Err(e) = self
          .gpu
          .configure_surface(&self.surface, size.width, size.height)
        {
          tracing::warn!("failed to reconfigure suboptimal wgpu surface: {e:#}");
        }
        Some(t)
      }
      CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
        let size = self.size.get();
        if let Err(e) = self
          .gpu
          .configure_surface(&self.surface, size.width, size.height)
        {
          tracing::warn!("failed to reconfigure outdated wgpu surface: {e:#}");
          return None;
        }
        match self.surface.get_current_texture() {
          CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => Some(t),
          _ => None,
        }
      }
      CurrentSurfaceTexture::Timeout
      | CurrentSurfaceTexture::Occluded
      | CurrentSurfaceTexture::Validation => None,
    }
  }
}

impl WindowAdapter for SlintWindow {
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
