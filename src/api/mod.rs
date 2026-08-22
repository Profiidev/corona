use crate::{Corona, event::event_loop::SendLoopEvent};

pub mod event;
pub mod hyprland;
pub mod widget;

#[derive(Clone)]
pub struct CoronaRemote(calloop::channel::Sender<SendLoopEvent>);

impl Corona {
  pub fn remote(&self) -> CoronaRemote {
    CoronaRemote(self.loop_handle().send_tx.clone())
  }
}

impl CoronaRemote {
  pub fn defer(&self, f: impl FnOnce(&Corona) + Send + 'static) {
    if self.0.send(Box::new(f)).is_err() {
      tracing::warn!("event loop already terminated, cannot defer event");
    }
  }

  pub fn spawn<T: Send + 'static>(
    &self,
    work: impl FnOnce() -> T + Send + 'static,
    then: impl FnOnce(&Corona, T) + Send + 'static,
  ) {
    let remote = self.clone();
    std::thread::spawn(move || {
      let value = work();
      remote.defer(move |corona| then(corona, value));
    });
  }
}
