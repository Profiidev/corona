//! [`slint::platform::WindowAdapter`] backed by femtovg + our EGL context. One per layer surface.
//! Ported from layer-shika's `FemtoVGWindow`.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use slint::platform::femtovg_renderer::FemtoVGRenderer;
use slint::platform::{Renderer, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, Window, WindowSize};

use crate::egl::EGLContext;

pub struct CoronaWindow {
  window: Window,
  renderer: FemtoVGRenderer,
  dirty: Cell<bool>,
  size: Cell<PhysicalSize>,
}

impl CoronaWindow {
  pub fn new(
    context: EGLContext,
    initial_size: PhysicalSize,
  ) -> Result<Rc<Self>, slint::PlatformError> {
    let renderer = FemtoVGRenderer::new(context)?;
    Ok(Rc::new_cyclic(|weak_self| {
      let window = Window::new(Weak::clone(weak_self) as Weak<dyn WindowAdapter>);
      Self {
        window,
        renderer,
        dirty: Cell::new(false),
        size: Cell::new(initial_size),
      }
    }))
  }

  /// Called after `WlSurface::commit`s configure ack, once the compositor tells us the real
  /// size — resizes the Slint window and marks it for a redraw.
  pub fn set_physical_size(&self, size: PhysicalSize) {
    self.size.set(size);
    self.window.dispatch_event(WindowEvent::Resized {
      size: size.to_logical(self.window.scale_factor()),
    });
    self.dirty.set(true);
  }

  /// Render if something changed since the last call. No-op otherwise (nothing to redraw).
  pub fn render_if_dirty(&self) -> Result<(), slint::PlatformError> {
    if self.dirty.replace(false) {
      self.renderer.render()?;
    }
    Ok(())
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
