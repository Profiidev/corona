use std::{cell::RefCell, rc::Rc};

use dashmap::DashMap;
use tracing::error;
use wayland_client::backend::ObjectId;

use crate::{
  adapter::{
    slint::{SlintCustomPlatform, SlintWindow},
    wayland::LayerSurfaceObjects,
  },
  error::CoronaError,
  widgets::init::{SlintComponent, SlintComponentInit},
};

pub mod init;

pub(crate) struct Widgets {
  active: DashMap<ObjectId, Widget>,
  pending: DashMap<ObjectId, PendingWidget>,
  /// Surface that currently holds the keyboard focus, if any.
  focus: RefCell<Option<ObjectId>>,
}

// field order is important for drop order
pub struct Widget {
  #[allow(dead_code)]
  component: Box<dyn SlintComponent>,
  pub(super) window: Rc<SlintWindow>,
}

pub(crate) struct PendingWidget {
  pub(crate) objects: LayerSurfaceObjects,
  pub(crate) init: Box<dyn SlintComponentInit>,
  pub(crate) scale: f64,
}

impl Widgets {
  pub fn new() -> Self {
    Self {
      active: DashMap::new(),
      pending: DashMap::new(),
      focus: RefCell::new(None),
    }
  }

  pub(crate) fn window(&self, id: &ObjectId) -> Option<Rc<SlintWindow>> {
    self.active.get(id).map(|widget| widget.window.clone())
  }

  pub fn set_scale(&self, id: &ObjectId, scale: f64) {
    if let Some(mut pending) = self.pending.get_mut(id) {
      pending.scale = scale;
    } else if let Some(widget) = self.active.get(id) {
      widget.window.set_scale(scale);
    }
  }

  pub fn needs_render(&self) -> bool {
    self
      .active
      .iter()
      .any(|widget| widget.window.needs_render())
  }

  pub fn frame_done(&self, id: &ObjectId) {
    if let Some(widget) = self.active.get(id) {
      widget.window.frame_done();
    }
  }

  pub fn render_if_dirty(&self) -> Result<(), CoronaError> {
    for widget in self.active.iter() {
      widget.window.render_if_dirty()?;
    }

    Ok(())
  }

  pub(crate) fn create_widget(
    &self,
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

  pub(crate) fn resize_widget(&self, id: ObjectId, width: u32, height: u32) {
    if let Some(widget) = self.active.get(&id) {
      widget.window.set_logical_size(width, height);
    }
  }

  pub(crate) fn take_pending(&self, id: &ObjectId) -> Option<PendingWidget> {
    self.pending.remove(id).map(|(_, pending)| pending)
  }

  pub fn create_pending_widget(
    &self,
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

  pub fn finish_widget_configure(
    &self,
    platform: &SlintCustomPlatform,
    id: ObjectId,
    width: u32,
    height: u32,
  ) {
    if let Some(mut pending) = self.take_pending(&id) {
      let window = match platform.create_window(pending.objects, width, height, pending.scale) {
        Ok(window) => window,
        Err(e) => {
          error!("Failed to create window for layer surface {}: {}", id, e);
          return;
        }
      };
      let Ok(component) = pending.init.init().map_err(|e| {
        error!(
          "Failed to initialize Slint component for layer surface {}: {}",
          id, e
        );
      }) else {
        return;
      };

      self.create_widget(id, window, component);
    } else {
      self.resize_widget(id, width, height);
    }
  }

  pub fn destroy_widget(&self, id: ObjectId) {
    self.active.remove(&id);
  }

  pub(crate) fn set_focus(&self, id: Option<ObjectId>) {
    *self.focus.borrow_mut() = id;
  }

  pub(crate) fn focus(&self) -> Option<ObjectId> {
    self.focus.borrow().clone()
  }
}

impl Widget {
  pub fn new(window: Rc<SlintWindow>, component: Box<dyn SlintComponent>) -> Self {
    Self { window, component }
  }
}
