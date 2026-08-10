use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use wayland_client::{Connection, QueueHandle, protocol::wl_seat};

use crate::Corona;

impl SeatHandler for Corona {
  fn seat_state(&mut self) -> &mut SeatState {
    self.wayland.seat_state_mut()
  }

  fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

  fn new_capability(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    seat: wl_seat::WlSeat,
    capability: Capability,
  ) {
    self.wayland.set_capability(&seat, capability, true);
  }

  fn remove_capability(
    &mut self,
    _conn: &Connection,
    _: &QueueHandle<Self>,
    seat: wl_seat::WlSeat,
    capability: Capability,
  ) {
    self.wayland.set_capability(&seat, capability, false);
  }

  fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
