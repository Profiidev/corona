use wayland_client::protocol::wl_output::WlOutput;

use crate::Corona;

impl Corona {
  pub fn outputs(&self) -> Vec<WlOutput> {
    self.wayland.output_state().outputs().collect::<Vec<_>>()
  }
}
