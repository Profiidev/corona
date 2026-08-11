use wayland_client::protocol::wl_output::WlOutput;

use crate::{Corona, event::event_loop::OnLoopEvent};

pub mod widget;

#[derive(Clone)]
pub struct CoronaHandle(calloop::channel::Sender<OnLoopEvent>);

impl Corona {
  pub fn outputs(&self) -> Vec<WlOutput> {
    self.wayland.output_state().outputs().collect::<Vec<_>>()
  }

  pub fn handle(&self) -> CoronaHandle {
    CoronaHandle(self.loop_handle.loop_tx.clone())
  }
}

impl CoronaHandle {
  pub fn defer(&self, f: impl FnOnce(&mut Corona) + 'static) {
    if self.0.send(Box::new(f)).is_err() {
      tracing::warn!("event loop already terminated, cannot defer event");
    }
  }
}
