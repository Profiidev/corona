use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use wayland_client::{Connection, QueueHandle, protocol::wl_seat};

use crate::Corona;

impl SeatHandler for Corona {
  fn seat_state(&mut self) -> &mut SeatState {
    self.wayland.seat_state_mut()
  }

  fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {
    // TODO
  }

  fn new_capability(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _seat: wl_seat::WlSeat,
    _capability: Capability,
  ) {
    // TODO
  }

  fn remove_capability(
    &mut self,
    _conn: &Connection,
    _: &QueueHandle<Self>,
    _: wl_seat::WlSeat,
    _capability: Capability,
  ) {
    // TODO
  }

  fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
