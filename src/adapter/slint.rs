use std::{
  cell::{Cell, RefCell},
  rc::{Rc, Weak},
};

use slint::{
  EventLoopError, PhysicalSize, PlatformError, Window, WindowSize,
  platform::{
    EventLoopProxy, Platform, Renderer, WindowAdapter, WindowEvent,
    femtovg_renderer::FemtoVGWGPURenderer,
  },
};
use smithay_client_toolkit::{compositor::FrameCallbackData, shell::WaylandSurface};
use wayland_client::Proxy;
use wgpu::CurrentSurfaceTexture;

use crate::{
  adapter::{gpu::GpuContext, wayland::LayerSurfaceObjects},
  event::event_loop::{EventLoop, SendLoopEvent},
};

pub struct SlintCustomPlatform {
  pending: RefCell<Option<Rc<SlintWindow>>>,
  gpu: Weak<GpuContext>,
  proxy: SlintEventLoopProxy,
}

#[derive(Clone)]
struct SlintEventLoopProxy(calloop::channel::Sender<SendLoopEvent>);

impl EventLoopProxy for SlintEventLoopProxy {
  fn quit_event_loop(&self) -> Result<(), EventLoopError> {
    self
      .0
      .send(Box::new(|state| {
        state.exit_requested = true;
      }))
      .map_err(|_| EventLoopError::EventLoopTerminated)
  }

  fn invoke_from_event_loop(&self, event: Box<dyn FnOnce() + Send>) -> Result<(), EventLoopError> {
    self
      .0
      .send(Box::new(|_| {
        event();
      }))
      .map_err(|_| EventLoopError::EventLoopTerminated)
  }
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
  #[error("GPU context is no longer available")]
  GpuNotAvailable,
}

impl SlintCustomPlatform {
  pub fn init(
    gpu: Rc<GpuContext>,
    event_loop: &EventLoop,
  ) -> Result<Rc<Self>, SlintCustomPlatformError> {
    let platform = Rc::new(Self {
      pending: RefCell::new(None),
      gpu: Rc::downgrade(&gpu),
      proxy: SlintEventLoopProxy(event_loop.send_sender()),
    });

    slint::platform::set_platform(Box::new(SlintCustomPlatformPointer(platform.clone())))
      .map_err(|_| SlintCustomPlatformError::PlatformAlreadySet)?;

    Ok(platform)
  }

  pub fn create_window(
    &self,
    objects: LayerSurfaceObjects,
    width: u32,
    height: u32,
    scale: f64,
  ) -> Result<Rc<SlintWindow>, SlintCustomPlatformError> {
    if self.pending.borrow().as_ref().is_some() {
      return Err(SlintCustomPlatformError::WindowAlreadyPending);
    }

    let Some(gpu) = self.gpu.upgrade() else {
      return Err(SlintCustomPlatformError::GpuNotAvailable);
    };

    let window = SlintWindow::new(gpu, objects, width, height, scale)?;
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

  fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
    Some(Box::new(self.proxy.clone()))
  }
}

impl Platform for SlintCustomPlatformPointer {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    self.0.create_window_adapter()
  }

  fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
    self.0.new_event_loop_proxy()
  }
}

// field order is important for drop order
pub struct SlintWindow {
  window: Window,
  renderer: FemtoVGWGPURenderer,
  surface: wgpu::Surface<'static>,
  objects: LayerSurfaceObjects,
  gpu: Rc<GpuContext>,
  dirty: Cell<bool>,
  /// True if  waiting for callback from compositor to draw next frame
  frame_pending: Cell<bool>,
  /// Size of the wgpu surface, in physical pixels: `round(logical * scale)`.
  size: Cell<PhysicalSize>,
  /// Size the compositor laid out, in logical pixels. Without scale applied.
  logical: Cell<(u32, u32)>,
  scale: Cell<f64>,
}

impl SlintWindow {
  fn new(
    gpu: Rc<GpuContext>,
    objects: LayerSurfaceObjects,
    width: u32,
    height: u32,
    scale: f64,
  ) -> Result<Rc<Self>, SlintCustomPlatformError> {
    let (physical_width, physical_height) = physical_size(width, height, scale);
    objects
      .viewport
      .set_destination(width as i32, height as i32);

    let surface = gpu.create_surface(
      &objects.layer_surface.wl_surface().id(),
      physical_width,
      physical_height,
    )?;
    tracing::debug!(
      "new window: scale {} logical {}x{} physical {}x{}",
      scale,
      width,
      height,
      physical_width,
      physical_height
    );

    let renderer =
      FemtoVGWGPURenderer::new(gpu.instance.clone(), gpu.device.clone(), gpu.queue.clone())?;

    Ok(Rc::new_cyclic(|weak_self| {
      let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);

      window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale as f32,
      });

      Self {
        window,
        renderer,
        surface,
        objects,
        gpu,
        dirty: Cell::new(false),
        frame_pending: Cell::new(false),
        size: Cell::new(PhysicalSize::new(physical_width, physical_height)),
        logical: Cell::new((width, height)),
        scale: Cell::new(scale),
      }
    }))
  }

  pub fn dispatch(&self, event: WindowEvent) {
    self.window.dispatch_event(event);
  }

  pub fn set_logical_size(&self, width: u32, height: u32) {
    if self.logical.replace((width, height)) == (width, height) {
      return;
    }
    self.apply_geometry(false);
  }

  pub fn set_scale(&self, scale: f64) {
    if self.scale.replace(scale) == scale {
      return;
    }
    self.apply_geometry(true);
  }

  fn apply_geometry(&self, scale_changed: bool) {
    let (logical_width, logical_height) = self.logical.get();
    let scale = self.scale.get();

    let (width, height) = physical_size(logical_width, logical_height, scale);
    let size = PhysicalSize::new(width, height);

    self.size.set(size);
    tracing::debug!(
      "geometry: scale {} logical {}x{} physical {}x{}",
      scale,
      logical_width,
      logical_height,
      width,
      height
    );

    self
      .objects
      .viewport
      .set_destination(logical_width as i32, logical_height as i32);

    if let Err(e) = self.gpu.configure_surface(&self.surface, width, height) {
      tracing::warn!("failed to reconfigure wgpu surface: {e:#}");
    }

    if scale_changed {
      self.window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale as f32,
      });
    }

    self.window.dispatch_event(WindowEvent::Resized {
      size: size.to_logical(self.window.scale_factor()),
    });
    self.dirty.set(true);
  }

  pub fn has_active_animations(&self) -> bool {
    self.window.has_active_animations()
  }

  pub fn frame_done(&self) {
    self.frame_pending.set(false);
  }

  pub fn needs_render(&self) -> bool {
    (self.dirty.get() || self.has_active_animations()) && !self.frame_pending.get()
  }

  pub fn render_if_dirty(&self) -> Result<(), SlintCustomPlatformError> {
    if self.has_active_animations() {
      self.dirty.set(true);
    }

    if self.frame_pending.get() {
      return Ok(());
    }

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

    self.objects.wl_surface.frame(
      &self.objects.qh,
      FrameCallbackData(self.objects.wl_surface.clone()),
    );
    self.frame_pending.set(true);

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
    // set from the compositor via layer shell configuration
  }

  fn request_redraw(&self) {
    self.dirty.set(true);
  }
}

fn physical_size(width: u32, height: u32, scale: f64) -> (u32, u32) {
  let px = |v: u32| ((v as f64 * scale).round() as u32).max(1);
  (px(width), px(height))
}
