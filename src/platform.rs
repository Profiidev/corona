//! Custom [`slint::platform::Platform`] handing out our layer-shell-backed window adapters.
//! No winit, no KMS — Slint's normal backends can't produce a `wlr-layer-shell` surface, so we
//! supply our own (see the plan doc for why). Ported from layer-shika's `CustomSlintPlatform`,
//! simplified: no popup support yet (single always-on surface for now).

use std::cell::RefCell;
use std::rc::Rc;

use slint::PlatformError;
use slint::platform::WindowAdapter;

use crate::window::CoronaWindow;

pub struct CoronaPlatform {
  pending: RefCell<Vec<Rc<CoronaWindow>>>,
}

impl CoronaPlatform {
  pub fn new() -> Rc<Self> {
    Rc::new(Self {
      pending: RefCell::new(Vec::new()),
    })
  }

  /// Register a window so the next `create_window_adapter()` call (triggered by the matching
  /// `slint::include_modules!` component's `::new()`) hands it out.
  pub fn add_window(&self, window: Rc<CoronaWindow>) {
    self.pending.borrow_mut().push(window);
  }
}

impl slint::platform::Platform for CoronaPlatform {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    self
      .pending
      .borrow_mut()
      .pop()
      .map(|w| w as Rc<dyn WindowAdapter>)
      .ok_or(PlatformError::NoPlatform)
  }
}
