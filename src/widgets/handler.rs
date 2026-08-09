use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use tracing::error;
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::Corona;

impl LayerShellHandler for Corona {
  fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
    // TODO
  }

  fn configure(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    layer: &LayerSurface,
    configure: LayerSurfaceConfigure,
    _serial: u32,
  ) {
    let id = layer.wl_surface().id();
    let (width, height) = configure.new_size;
    let width = width.max(1);
    let height = height.max(1);

    if let Some(mut pending) = self.widgets.pending.remove(&id) {
      let window = match self
        .platform
        .create_window(pending.layer_surface, width, height)
      {
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

      self
        .widgets
        .create_widget(id, window, pending.kind, component);
    } else {
      self.widgets.resize_widget(id, width, height);
    }
  }
}
