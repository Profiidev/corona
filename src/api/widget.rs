use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{Anchor, KeyboardInteractivity, Layer},
};
use wayland_client::{Proxy, backend::ObjectId, protocol::wl_output::WlOutput};

use crate::{Corona, adapter::wayland::LayerSurfaceSpec, widgets::init::IntoSlintInit};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetHandle(ObjectId);

pub struct WidgetBuilder<'c> {
  corona: &'c mut Corona,
  namespace: String,
  layer: Layer,
  anchor: Anchor,
  width: u32,
  height: u32,
  exclusive_zone: i32,
  keyboard_interactivity: KeyboardInteractivity,
}

#[derive(Debug, thiserror::Error)]
pub enum WidgetError {
  #[error("Width must be greater than 0 when not anchoring to both left and right")]
  InvalidWidth,
  #[error("Height must be greater than 0 when not anchoring to both top and bottom")]
  InvalidHeight,
}

impl Corona {
  pub fn widget_builder<'c>(&'c mut self) -> WidgetBuilder<'c> {
    WidgetBuilder {
      corona: self,
      namespace: "default".into(),
      layer: Layer::Top,
      anchor: Anchor::empty(),
      width: 0,
      height: 0,
      exclusive_zone: 0,
      keyboard_interactivity: KeyboardInteractivity::None,
    }
  }

  pub fn destroy_widget(&mut self, handle: WidgetHandle) {
    self.widgets.destroy_widget(handle.0);
  }
}

impl WidgetBuilder<'_> {
  pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
    self.namespace = namespace.into();
    self
  }

  pub fn layer(mut self, layer: Layer) -> Self {
    self.layer = layer;
    self
  }

  pub fn anchor(mut self, anchor: Anchor) -> Self {
    self.anchor = anchor;
    self
  }

  pub fn width(mut self, width: u32) -> Self {
    self.width = width;
    self
  }

  pub fn height(mut self, height: u32) -> Self {
    self.height = height;
    self
  }

  pub fn exclusive_zone(mut self, exclusive_zone: i32) -> Self {
    self.exclusive_zone = exclusive_zone;
    self
  }

  pub fn keyboard_interactivity(mut self, keyboard_interactivity: KeyboardInteractivity) -> Self {
    self.keyboard_interactivity = keyboard_interactivity;
    self
  }

  pub fn build<C>(
    self,
    output: &WlOutput,
    init: impl IntoSlintInit<C>,
  ) -> Result<WidgetHandle, WidgetError> {
    if self.width == 0 && !self.anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
      return Err(WidgetError::InvalidWidth);
    }

    if self.height == 0 && !self.anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
      return Err(WidgetError::InvalidHeight);
    }

    let objects = self.corona.wayland.create_layer_surface(LayerSurfaceSpec {
      namespace: self.namespace,
      layer: self.layer,
      anchor: self.anchor,
      width: self.width,
      height: self.height,
      exclusive_zone: self.exclusive_zone,
      output: Some(output),
      keyboard_interactivity: self.keyboard_interactivity,
    });
    let id = objects.layer_surface.wl_surface().id();

    self
      .corona
      .widgets
      .create_pending_widget(id.clone(), objects, Box::new(init.into_init()), 1.0);

    Ok(WidgetHandle(id))
  }
}
