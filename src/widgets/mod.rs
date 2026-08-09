use std::{collections::HashMap, rc::Rc};

use slint::PhysicalSize;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::backend::ObjectId;

use crate::{
  adapter::slint::SlintWindow,
  error::CoronaError,
  widgets::{
    init::{SlintComponent, SlintComponentInit},
    widget::Widget,
  },
};

mod handler;
pub mod init;
mod widget;

pub struct Widgets {
  live: HashMap<ObjectId, Widget>,
  pending: HashMap<ObjectId, PendingWidget>,
}

struct PendingWidget {
  layer_surface: LayerSurface,
  init: Box<dyn SlintComponentInit>,
}

impl Widgets {
  pub fn new() -> Self {
    Self {
      live: HashMap::new(),
      pending: HashMap::new(),
    }
  }

  pub fn render_if_dirty(&self) -> Result<(), CoronaError> {
    for widget in self.live.values() {
      widget.window.render_if_dirty()?;
    }

    Ok(())
  }

  fn create_widget(
    &mut self,
    id: ObjectId,
    window: Rc<SlintWindow>,
    component: Box<dyn SlintComponent>,
  ) {
    self.live.insert(id, Widget { window, component });
  }

  fn resize_widget(&mut self, id: ObjectId, width: u32, height: u32) {
    if let Some(widget) = self.live.get_mut(&id) {
      widget
        .window
        .set_physical_size(PhysicalSize::new(width, height));
    }
  }
}
