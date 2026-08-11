use std::{collections::HashMap, rc::Rc};

use wayland_client::backend::ObjectId;

use crate::{
  adapter::{slint::SlintWindow, wayland::LayerSurfaceObjects},
  error::CoronaError,
  widgets::init::{SlintComponent, SlintComponentInit},
};

mod handler;
pub mod init;

pub(crate) struct Widgets {
  active: HashMap<ObjectId, Widget>,
  pending: HashMap<ObjectId, PendingWidget>,
  /// Surface that currently holds the keyboard focus, if any.
  pub(crate) focus: Option<ObjectId>,
}

// field order is important for drop order
pub struct Widget {
  #[allow(dead_code)]
  component: Box<dyn SlintComponent>,
  pub(super) window: Rc<SlintWindow>,
}

struct PendingWidget {
  objects: LayerSurfaceObjects,
  init: Box<dyn SlintComponentInit>,
  scale: f64,
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

  pub fn set_scale(&mut self, id: &ObjectId, scale: f64) {
    if let Some(pending) = self.pending.get_mut(id) {
      pending.scale = scale;
    } else if let Some(widget) = self.active.get(id) {
      widget.window.set_scale(scale);
    }
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
      widget.window.set_logical_size(width, height);
    }
  }

  pub fn create_pending_widget(
    &mut self,
    id: ObjectId,
    objects: LayerSurfaceObjects,
    init: Box<dyn SlintComponentInit>,
    scale: f64,
  ) {
    self.pending.insert(
      id,
      PendingWidget {
        objects,
        init,
        scale,
      },
    );
  }

  pub fn destroy_widget(&mut self, id: ObjectId) {
    self.active.remove(&id);
  }
}

impl Widget {
  pub fn new(window: Rc<SlintWindow>, component: Box<dyn SlintComponent>) -> Self {
    Self { window, component }
  }
}
