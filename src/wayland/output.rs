use smithay_client_toolkit::output::{OutputHandler, OutputState};
use wayland_client::{Connection, QueueHandle, protocol::wl_output};

use crate::{Corona, api::event::ShellEvent, event::event::OutputEvent};

impl OutputHandler for Corona {
  fn output_state(&mut self) -> &mut OutputState {
    self.wayland.output_state_mut()
  }

  fn new_output(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    output: wl_output::WlOutput,
  ) {
    self.handle_shell_event(ShellEvent::Output(OutputEvent::New(output)));
  }

  fn update_output(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    output: wl_output::WlOutput,
  ) {
    self.handle_shell_event(ShellEvent::Output(OutputEvent::Update(output)));
  }

  fn output_destroyed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    output: wl_output::WlOutput,
  ) {
    self.handle_shell_event(ShellEvent::Output(OutputEvent::Destroy(output)));
  }
}
