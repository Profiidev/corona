use std::{cell::RefCell, rc::Rc};

use slint::{
  PhysicalSize, PlatformError,
  platform::{Platform, WindowAdapter},
};

pub struct SlintCustomPlatform {
  pending: RefCell<Vec<Rc<SlintWindow>>>,
}

impl SlintCustomPlatform {
  pub fn new() -> Rc<Self> {
    Rc::new(Self {
      pending: RefCell::new(Vec::new()),
    })
  }

  fn add_window(&self, window: Rc<SlintWindow>) {
    self.pending.borrow_mut().push(window);
  }
}

impl Platform for SlintCustomPlatform {
  fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
    unimplemented!()
  }
}

struct SlintWindow {}
