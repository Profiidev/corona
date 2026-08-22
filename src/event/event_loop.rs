use std::time::{Duration, SystemTime, UNIX_EPOCH};

use calloop::{
  channel::Event,
  timer::{TimeoutAction, Timer},
};

use crate::{
  Corona, adapter::wayland::WaylandAdapter, event::event::ShellEvent, wayland::Dispatcher,
};

pub struct EventLoop {
  calloop: calloop::EventLoop<'static, Dispatcher>,
  event_tx: ShellSender,
  send_tx: calloop::channel::Sender<SendLoopEvent>,
}

pub struct LoopHandle {
  pub handle: calloop::LoopHandle<'static, Dispatcher>,
  pub send_tx: calloop::channel::Sender<SendLoopEvent>,
}

pub type ShellSender = calloop::channel::Sender<ShellEvent>;
pub type SendLoopEvent = Box<dyn FnOnce(&Corona) + Send + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
  #[error("Calloop error: {0}")]
  Calloop(#[from] calloop::Error),
  #[error("Wayland EventSource already taken")]
  WaylandEventSourceTaken,
  #[error("Slint event loop already taken")]
  SlintEventLoopTaken,
}

fn until_next_second() -> Duration {
  let subsec = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .subsec_nanos() as u64;
  // 1ms skew do prevent double trigger
  Duration::from_nanos(1_000_000_000 - subsec) + Duration::from_millis(1)
}

impl EventLoop {
  pub fn init(wayland: &mut WaylandAdapter) -> Result<Self, EventLoopError> {
    let calloop = calloop::EventLoop::<'static, Dispatcher>::try_new()?;
    let (tx, rx) = calloop::channel::channel::<ShellEvent>();

    wayland
      .event_source()
      .ok_or(EventLoopError::WaylandEventSourceTaken)?
      .insert(calloop.handle())
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    calloop
      .handle()
      .insert_source(rx, |event, _, state: &mut Dispatcher| {
        if let Event::Msg(event) = event {
          state.corona.handle_shell_event(event);
        }
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    let (send_tx, send_rx) = calloop::channel::channel::<SendLoopEvent>();
    calloop
      .handle()
      .insert_source(send_rx, |event, _, state: &mut Dispatcher| {
        if let Event::Msg(f) = event {
          f(&state.corona);
        }
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    let clock_timer = Timer::from_duration(until_next_second());
    calloop
      .handle()
      .insert_source(clock_timer, |_, _, state: &mut Dispatcher| {
        state.corona.handle_shell_event(ShellEvent::Tick);
        TimeoutAction::ToDuration(until_next_second())
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    Ok(Self {
      calloop,
      event_tx: tx,
      send_tx,
    })
  }

  pub fn send_sender(&self) -> calloop::channel::Sender<SendLoopEvent> {
    self.send_tx.clone()
  }

  pub fn dispatch(
    &mut self,
    state: &mut Dispatcher,
    timeout: Option<Duration>,
  ) -> Result<(), EventLoopError> {
    self
      .calloop
      .dispatch(timeout, state)
      .map_err(EventLoopError::Calloop)
  }

  pub fn event_sender(&self) -> ShellSender {
    self.event_tx.clone()
  }

  pub fn handle(&self) -> LoopHandle {
    LoopHandle {
      handle: self.calloop.handle(),
      send_tx: self.send_tx.clone(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lands_after_a_second_boundary() {
    for _ in 0..100 {
      let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
      let target = now + until_next_second();
      assert_eq!(target.as_secs(), now.as_secs() + 1);
      assert!(target.subsec_nanos() < 2_000_000, "{target:?}");
      std::thread::sleep(Duration::from_millis(7));
    }
  }
}
