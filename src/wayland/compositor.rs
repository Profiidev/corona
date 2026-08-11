use smithay_client_toolkit::compositor::CompositorHandler;
use wayland_client::{
  Connection, Proxy, QueueHandle,
  protocol::{wl_output, wl_surface},
};

use crate::Corona;

impl CompositorHandler for Corona {
  fn scale_factor_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _new_factor: i32,
  ) {
    // handled by wp_fractional_scale_v1
  }

  fn transform_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _new_transform: wl_output::Transform,
  ) {
    // handled by wp_fractional_scale_v1
  }

  fn frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    surface: &wl_surface::WlSurface,
    _time: u32,
  ) {
    self.widgets.frame_done(&surface.id());
  }

  fn surface_enter(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
    // currently not needed
  }

  fn surface_leave(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
    // currently not needed
  }
}
