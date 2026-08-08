use smithay_client_toolkit::shell::wlr_layer::{
  LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use wayland_client::{Connection, QueueHandle};

use crate::Corona;

impl LayerShellHandler for Corona {
  fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

  fn configure(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _layer: &LayerSurface,
    _configure: LayerSurfaceConfigure,
    _serial: u32,
  ) {
    // TODO
  }
}
