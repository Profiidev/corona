use std::{collections::HashMap, rc::Rc};

use slint::PhysicalSize;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::backend::ObjectId;

use crate::{adapter::slint::SlintWindow, error::CoronaError, widgets::bar::Bar};

mod bar;
mod handler;

pub struct Widgets {
  bars: HashMap<ObjectId, Bar>,
  pending: HashMap<ObjectId, PendingWidget>,
}

struct PendingWidget {
  layer_surface: LayerSurface,
  kind: WidgetKind,
}

enum WidgetKind {
  Bar,
}

impl Widgets {
  pub fn new() -> Self {
    Self {
      bars: HashMap::new(),
      pending: HashMap::new(),
    }
  }

  pub fn render_if_dirty(&self) -> Result<(), CoronaError> {
    for bar in self.bars.values() {
      bar.window.render_if_dirty()?;
    }

    Ok(())
  }

  fn create_widget(&mut self, id: ObjectId, window: Rc<SlintWindow>, kind: WidgetKind) {
    match kind {
      WidgetKind::Bar => {
        self.bars.insert(id, Bar { window });
      }
    }
  }

  fn resize_widget(&mut self, id: ObjectId, width: u32, height: u32) {
    if let Some(bar) = self.bars.get_mut(&id) {
      bar
        .window
        .set_physical_size(PhysicalSize::new(width, height));
    }
  }
}
