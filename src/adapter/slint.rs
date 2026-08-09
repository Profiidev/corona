use std::{
  cell::{Cell, RefCell},
  mem::ManuallyDrop,
  rc::{Rc, Weak},
};

#[cfg(feature = "hot-reload")]
use slint::{EventLoopError, platform::EventLoopProxy};
use slint::{
  PhysicalSize, PlatformError, Window, WindowSize,
  platform::{
    Platform, Renderer, WindowAdapter, WindowEvent, femtovg_renderer::FemtoVGWGPURenderer,
  },
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::Proxy;
use wgpu::CurrentSurfaceTexture;

#[cfg(feature = "hot-reload")]
use crate::Corona;
use crate::adapter::gpu::GpuContext;

#[cfg(feature = "hot-reload")]
pub type SlintOnLoopEvent = Box<dyn FnOnce(&mut Corona) + Send>;

pub struct SlintCustomPlatform {
  pending: RefCell<Option<Rc<SlintWindow>>>,
  gpu: Weak<GpuContext>,
  #[cfg(feature = "hot-reload")]
  loop_tx: calloop::channel::Sender<SlintOnLoopEvent>,
  #[cfg(feature = "hot-reload")]
  loop_rx: RefCell<Option<calloop::channel::Channel<SlintOnLoopEvent>>>,
}

#[cfg(feature = "hot-reload")]
struct SlintEventLoopProxy(calloop::channel::Sender<SlintOnLoopEvent>);

#[cfg(feature = "hot-reload")]
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
  pub fn init(gpu: Rc<GpuContext>) -> Result<Rc<Self>, SlintCustomPlatformError> {
    #[cfg(feature = "hot-reload")]
    let (loop_tx, loop_rx) = calloop::channel::channel::<SlintOnLoopEvent>();
    let platform = Rc::new(Self {
      pending: RefCell::new(None),
      gpu: Rc::downgrade(&gpu),
      #[cfg(feature = "hot-reload")]
      loop_tx,
      #[cfg(feature = "hot-reload")]
      loop_rx: RefCell::new(Some(loop_rx)),
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

    let Some(gpu) = self.gpu.upgrade() else {
      return Err(SlintCustomPlatformError::GpuNotAvailable);
    };

    let window = SlintWindow::new(gpu, layer_surface, width, height)?;
    self.pending.borrow_mut().replace(window.clone());

    Ok(window)
  }

  #[cfg(feature = "hot-reload")]
  pub fn event_source(&self) -> Option<calloop::channel::Channel<SlintOnLoopEvent>> {
    self.loop_rx.take()
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

  #[cfg(feature = "hot-reload")]
  fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
    Some(Box::new(SlintEventLoopProxy(self.loop_tx.clone())))
  }
}

impl Platform for SlintCustomPlatformPointer {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    self.0.create_window_adapter()
  }

  #[cfg(feature = "hot-reload")]
  fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
    self.0.new_event_loop_proxy()
  }
}

pub struct SlintWindow {
  window: ManuallyDrop<Window>,
  renderer: ManuallyDrop<FemtoVGWGPURenderer>,
  surface: ManuallyDrop<wgpu::Surface<'static>>,
  gpu: Rc<GpuContext>,
  dirty: Cell<bool>,
  size: Cell<PhysicalSize>,
  layer_surface: ManuallyDrop<LayerSurface>,
}

impl SlintWindow {
  fn new(
    gpu: Rc<GpuContext>,
    layer_surface: LayerSurface,
    width: u32,
    height: u32,
  ) -> Result<Rc<Self>, SlintCustomPlatformError> {
    let surface = gpu.create_surface(&layer_surface.wl_surface().id(), width, height)?;
    let renderer =
      FemtoVGWGPURenderer::new(gpu.instance.clone(), gpu.device.clone(), gpu.queue.clone())?;

    Ok(Rc::new_cyclic(|weak_self| {
      let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);

      Self {
        window: ManuallyDrop::new(window),
        renderer: ManuallyDrop::new(renderer),
        surface: ManuallyDrop::new(surface),
        gpu,
        dirty: Cell::new(false),
        size: Cell::new(PhysicalSize::new(width, height)),
        layer_surface: ManuallyDrop::new(layer_surface),
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
    &*self.renderer
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

impl Drop for SlintWindow {
  fn drop(&mut self) {
    unsafe {
      ManuallyDrop::drop(&mut self.window);
      ManuallyDrop::drop(&mut self.renderer);
      ManuallyDrop::drop(&mut self.surface);
      ManuallyDrop::drop(&mut self.layer_surface);
    }
  }
}
