use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use wayland_client::{Connection, Proxy, QueueHandle};

use super::Dispatcher;

impl LayerShellHandler for Dispatcher {
  fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
    let id = layer.wl_surface().id();
    self.corona.widgets().destroy_widget(id);
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

    self
      .corona
      .widgets()
      .finish_widget_configure(self.corona.platform(), id, width, height);
  }
}
