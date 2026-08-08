use std::time::Duration;

use calloop::{
  channel::Event,
  timer::{TimeoutAction, Timer},
};

use crate::{Corona, adapter::wayland::WaylandAdapter, event::event::ShellEvent};

pub struct EventLoop {
  calloop: calloop::EventLoop<'static, Corona>,
}

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
  #[error("Calloop error: {0}")]
  Calloop(#[from] calloop::Error),
  #[error("Wayland EventSource already taken")]
  WaylandEventSourceTaken,
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

    let clock_timer = Timer::from_duration(Duration::from_secs(1));
    calloop
      .handle()
      .insert_source(clock_timer, |_, _, state: &mut Corona| {
        state.tick_clock();
        TimeoutAction::ToDuration(Duration::from_secs(1))
      })
      .map_err(|e| EventLoopError::Calloop(e.error))?;

    // TODO add additional event source for Hyprland IPC, D-Bus, hot-reload watcher, etc.
    let _ = tx;

    Ok(Self { calloop })
  }

  pub fn dispatch(&mut self, state: &mut Corona) -> Result<(), EventLoopError> {
    self
      .calloop
      .dispatch(None, state)
      .map_err(EventLoopError::Calloop)
  }
}
