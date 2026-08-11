use std::time::{Duration, SystemTime, UNIX_EPOCH};

use calloop::{
  channel::Event,
  timer::{TimeoutAction, Timer},
};

use crate::{Corona, adapter::wayland::WaylandAdapter, event::event::ShellEvent};

pub struct EventLoop {
  calloop: calloop::EventLoop<'static, Corona>,
  event_tx: ShellSender,
  loop_tx: calloop::channel::Sender<OnLoopEvent>,
  #[cfg(feature = "hot-reload")]
  slint_tx: calloop::channel::Sender<SlintOnLoopEvent>,
}

pub struct LoopHandle {
  pub handle: calloop::LoopHandle<'static, Corona>,
  pub loop_tx: calloop::channel::Sender<OnLoopEvent>,
}

pub type OnLoopEvent = Box<dyn FnOnce(&mut Corona) + 'static>;
pub type ShellSender = calloop::channel::Sender<ShellEvent>;

#[cfg(feature = "hot-reload")]
pub type SlintOnLoopEvent = Box<dyn FnOnce(&mut Corona) + Send + 'static>;

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
    let calloop = calloop::EventLoop::<'static, Corona>::try_new()?;
    let (tx, rx) = calloop::channel::channel::<ShellEvent>();

    wayland
      .event_source()
      .ok_or(EventLoopError::WaylandEventSourceTaken)?
      .insert(calloop.handle())
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    calloop
      .handle()
      .insert_source(rx, |event, _, state: &mut Corona| {
        if let Event::Msg(event) = event {
          state.handle_shell_event(event);
        }
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    let (loop_tx, loop_rx) = calloop::channel::channel::<OnLoopEvent>();
    calloop
      .handle()
      .insert_source(loop_rx, |event, _, state: &mut Corona| {
        if let Event::Msg(f) = event {
          f(state);
        }
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    #[cfg(feature = "hot-reload")]
    let slint_tx = {
      let (slint_tx, slint_rx) = calloop::channel::channel::<SlintOnLoopEvent>();
      calloop
        .handle()
        .insert_source(slint_rx, |event, _, state: &mut Corona| {
          if let Event::Msg(f) = event {
            f(state);
          }
        })
        .map_err(|e| EventLoopError::Calloop(e.error))?;
      slint_tx
    };

    let clock_timer = Timer::from_duration(until_next_second());
    calloop
      .handle()
      .insert_source(clock_timer, |_, _, state: &mut Corona| {
        state.handle_shell_event(ShellEvent::Tick);
        TimeoutAction::ToDuration(until_next_second())
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    Ok(Self {
      calloop,
      loop_tx,
      event_tx: tx,
      #[cfg(feature = "hot-reload")]
      slint_tx,
    })
  }

  #[cfg(feature = "hot-reload")]
  pub fn slint_sender(&self) -> calloop::channel::Sender<SlintOnLoopEvent> {
    self.slint_tx.clone()
  }

  pub fn dispatch(
    &mut self,
    state: &mut Corona,
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
      loop_tx: self.loop_tx.clone(),
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
