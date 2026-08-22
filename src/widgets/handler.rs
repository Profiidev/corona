use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use tracing::error;
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::wayland::Dispatcher;

impl LayerShellHandler for Dispatcher {
  fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
    let id = layer.wl_surface().id();
    self.corona.widgets.destroy_widget(id);
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

    if let Some((_, mut pending)) = self.corona.widgets.pending.remove(&id) {
      let window =
        match self
          .corona
          .platform
          .create_window(pending.objects, width, height, pending.scale)
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

      self.corona.widgets.create_widget(id, window, component);
    } else {
      self.corona.widgets.resize_widget(id, width, height);
    }
  }
}
