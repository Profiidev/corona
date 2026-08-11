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
pub use widget::{WidgetBuilder, WidgetError, WidgetHandle};

pub(crate) struct Widgets {
  active: HashMap<ObjectId, Widget>,
  pending: HashMap<ObjectId, PendingWidget>,
  /// Surface that currently holds the keyboard focus, if any.
  pub(crate) focus: Option<ObjectId>,
}

struct PendingWidget {
  layer_surface: LayerSurface,
  init: Box<dyn SlintComponentInit>,
}

impl Widgets {
  pub fn new() -> Self {
    Self {
      active: HashMap::new(),
      pending: HashMap::new(),
      focus: None,
    }
  }

  pub(crate) fn window(&self, id: &ObjectId) -> Option<&Rc<SlintWindow>> {
    self.active.get(id).map(|widget| &widget.window)
  }

  pub fn has_active_animations(&self) -> bool {
    self
      .active
      .values()
      .any(|widget| widget.window.has_active_animations())
  }

  pub fn render_if_dirty(&self) -> Result<(), CoronaError> {
    for widget in self.active.values() {
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
    let widget = Widget::new(window, component);

    // do an initial paint to ensure the widget is visible immediately
    if let Err(e) = widget.window.render_if_dirty() {
      tracing::warn!("failed to paint new widget {}: {}", id, e);
    }

    self.active.insert(id, widget);
  }

  fn resize_widget(&mut self, id: ObjectId, width: u32, height: u32) {
    if let Some(widget) = self.active.get_mut(&id) {
      widget
        .window
        .set_physical_size(PhysicalSize::new(width, height));
    }
  }

  fn destroy_widget(&mut self, id: ObjectId) {
    self.active.remove(&id);
  }
}
