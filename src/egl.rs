//! EGL context/surface management for rendering Slint via femtovg on wlr-layer-shell surfaces.
//!
//! One [`RenderContextManager`] is shared process-wide: it holds an EGLDisplay and an EGLConfig,
//! plus a surfaceless "root" context. Every layer surface gets its own [`EGLContext`] created via
//! [`RenderContextFactory`], sharing GL objects (textures, shaders) with the root context so
//! femtovg's font/texture caches aren't duplicated per surface.
//!
//! Ported from layer-shika's `crates/adapters/src/rendering/egl` (same approach, single error type).

use std::ffi::{CStr, c_void};
use std::num::NonZeroU32;
use std::ptr::NonNull;
use std::rc::Rc;

use anyhow::{Context, Result};
use glutin::api::egl::config::Config;
use glutin::api::egl::context::PossiblyCurrentContext;
use glutin::api::egl::display::Display;
use glutin::api::egl::surface::Surface;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::ContextAttributesBuilder;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{
  RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use slint::platform::femtovg_renderer::OpenGLInterface;
use wayland_client::backend::ObjectId;

/// Holds the EGLDisplay + EGLConfig shared by every surface, and a surfaceless root context that
/// every per-surface context shares GL objects with.
pub struct RenderContextManager {
  display: Display,
  config: Config,
  root_context: PossiblyCurrentContext,
}

impl RenderContextManager {
  pub fn new(display_id: &ObjectId) -> Result<Rc<Self>> {
    let display = unsafe { Display::new(wayland_display_handle(display_id)?) }
      .context("failed to create EGL display")?;
    let config = select_config(&display)?;

    let not_current =
      unsafe { display.create_context(&config, &ContextAttributesBuilder::default().build(None)) }
        .context("failed to create root EGL context")?;
    let root_context = not_current
      .make_current_surfaceless()
      .context("failed to make root EGL context current (surfaceless)")?;

    Ok(Rc::new(Self {
      display,
      config,
      root_context,
    }))
  }
}

/// Creates per-surface [`EGLContext`]s that share GL objects with the [`RenderContextManager`]'s
/// root context.
pub struct RenderContextFactory {
  manager: Rc<RenderContextManager>,
}

impl RenderContextFactory {
  pub fn new(manager: Rc<RenderContextManager>) -> Rc<Self> {
    Rc::new(Self { manager })
  }

  pub fn create_context(
    &self,
    surface_id: &ObjectId,
    width: u32,
    height: u32,
  ) -> Result<EGLContext> {
    let context_attributes =
      ContextAttributesBuilder::default().with_sharing(&self.manager.root_context);

    let not_current = unsafe {
      self
        .manager
        .display
        .create_context(&self.manager.config, &context_attributes.build(None))
    }
    .context("failed to create EGL context")?;

    let surface = create_surface(
      &self.manager.display,
      &self.manager.config,
      surface_id,
      width,
      height,
    )?;

    let context = not_current
      .make_current(&surface)
      .context("failed to make EGL context current")?;

    Ok(EGLContext { surface, context })
  }
}

/// A per-surface EGL context + window surface, implementing femtovg's [`OpenGLInterface`] so
/// Slint's `FemtoVGRenderer` can drive it directly.
pub struct EGLContext {
  surface: Surface<WindowSurface>,
  context: PossiblyCurrentContext,
}

impl EGLContext {
  fn ensure_current(&self) -> Result<()> {
    if !self.context.is_current() {
      self
        .context
        .make_current(&self.surface)
        .context("failed to make EGL context current")?;
    }
    Ok(())
  }
}

unsafe impl OpenGLInterface for EGLContext {
  fn ensure_current(&self) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Ok(self.ensure_current()?)
  }

  fn swap_buffers(&self) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Ok(self.surface.swap_buffers(&self.context)?)
  }

  fn resize(
    &self,
    width: NonZeroU32,
    height: NonZeroU32,
  ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    self.ensure_current()?;
    self.surface.resize(&self.context, width, height);
    Ok(())
  }

  fn get_proc_address(&self, name: &CStr) -> *const c_void {
    self.context.display().get_proc_address(name)
  }
}

fn wayland_display_handle(display_id: &ObjectId) -> Result<RawDisplayHandle> {
  let ptr =
    NonNull::new(display_id.as_ptr().cast::<c_void>()).context("wl_display pointer was null")?;
  Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr)))
}

fn wayland_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle> {
  let ptr =
    NonNull::new(surface_id.as_ptr().cast::<c_void>()).context("wl_surface pointer was null")?;
  Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr)))
}

fn select_config(display: &Display) -> Result<Config> {
  let mut configs = unsafe { display.find_configs(ConfigTemplateBuilder::default().build()) }
    .context("failed to enumerate EGL configs")?;
  configs.next().context("no compatible EGL config found")
}

fn create_surface(
  display: &Display,
  config: &Config,
  surface_id: &ObjectId,
  width: u32,
  height: u32,
) -> Result<Surface<WindowSurface>> {
  let handle = wayland_surface_handle(surface_id)?;
  let width = NonZeroU32::new(width).context("surface width was zero")?;
  let height = NonZeroU32::new(height).context("surface height was zero")?;
  let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(handle, width, height);
  unsafe { display.create_window_surface(config, &attrs) }
    .context("failed to create EGL window surface")
}
