use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerHandler};
use wayland_client::{Connection, QueueHandle, protocol::wl_pointer};

use crate::Corona;

impl PointerHandler for Corona {
  fn pointer_frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _pointer: &wl_pointer::WlPointer,
    _events: &[PointerEvent],
  ) {
    // TODO
  }
}
