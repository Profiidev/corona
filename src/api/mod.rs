use wayland_client::protocol::wl_output::WlOutput;

use crate::{
  Corona,
  event::event_loop::{OnLoopEvent, SendLoopEvent},
};

pub mod event;
pub mod hyprland;
pub mod widget;

#[derive(Clone)]
pub struct CoronaHandle(calloop::channel::Sender<OnLoopEvent>);

#[derive(Clone)]
pub struct CoronaRemote(calloop::channel::Sender<SendLoopEvent>);

impl Corona {
  pub fn outputs(&self) -> Vec<WlOutput> {
    self.wayland.output_state().outputs().collect::<Vec<_>>()
  }

  pub fn handle(&self) -> CoronaHandle {
    CoronaHandle(self.loop_handle.loop_tx.clone())
  }

  pub fn remote(&self) -> CoronaRemote {
    CoronaRemote(self.loop_handle.send_tx.clone())
  }
}

impl CoronaHandle {
  pub fn defer(&self, f: impl FnOnce(&mut Corona) + 'static) {
    if self.0.send(Box::new(f)).is_err() {
      tracing::warn!("event loop already terminated, cannot defer event");
    }
  }
}

impl CoronaRemote {
  pub fn defer(&self, f: impl FnOnce(&mut Corona) + Send + 'static) {
    if self.0.send(Box::new(f)).is_err() {
      tracing::warn!("event loop already terminated, cannot defer event");
    }
  }

  pub fn spawn<T: Send + 'static>(
    &self,
    work: impl FnOnce() -> T + Send + 'static,
    then: impl FnOnce(&mut Corona, T) + Send + 'static,
  ) {
    let remote = self.clone();
    std::thread::spawn(move || {
      let value = work();
      remote.defer(move |corona| then(corona, value));
    });
  }
}
