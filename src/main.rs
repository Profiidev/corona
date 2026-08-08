mod adapter;
mod app;
mod bar_ui;
mod dbus;
mod events;
mod gpu;
#[cfg(feature = "hot-reload")]
mod hotreload;
mod hyprland_ipc;
mod platform;
mod surface;
mod ui;
mod window;

use std::time::Duration;

use anyhow::{Context, Result};
use app::AppState;
use calloop::EventLoop;
use calloop::timer::{TimeoutAction, Timer};
use calloop_wayland_source::WaylandSource;
use events::ShellEvent;
use wayland_client::Connection;
use wayland_client::globals::registry_queue_init;

fn main() -> Result<()> {
  tracing_subscriber::fmt::init();

  let conn =
    Connection::connect_to_env().context("no Wayland compositor found (is Hyprland running?)")?;
  let (globals, event_queue) =
    registry_queue_init::<AppState>(&conn).context("failed to enumerate Wayland globals")?;
  let qh = event_queue.handle();

  let mut event_loop: EventLoop<'static, AppState> =
    EventLoop::try_new().context("failed to create event loop")?;

  // Off-thread event sources (Hyprland IPC, D-Bus, the hot-reload watcher, UI click callbacks)
  // all funnel into the single-threaded event loop through this channel — AppState only ever
  // mutates from here.
  let (tx, rx) = calloop::channel::channel::<ShellEvent>();

  let mut state = AppState::new(&conn, &qh, event_loop.handle(), tx.clone(), &globals)?;

  WaylandSource::new(conn.clone(), event_queue)
    .insert(event_loop.handle())
    .map_err(|e| anyhow::anyhow!("failed to insert Wayland source into event loop: {e}"))?;

  event_loop
    .handle()
    .insert_source(rx, |event, _, state: &mut AppState| {
      if let calloop::channel::Event::Msg(event) = event {
        state.handle_shell_event(event);
      }
    })
    .map_err(|e| anyhow::anyhow!("failed to insert shell-event channel into event loop: {e}"))?;

  hyprland_ipc::spawn(tx.clone());
  dbus::spawn(tx.clone());
  #[cfg(feature = "hot-reload")]
  hotreload::spawn(tx);

  let clock_timer = Timer::from_duration(Duration::from_secs(1));
  event_loop
    .handle()
    .insert_source(clock_timer, |_, _, state: &mut AppState| {
      state.tick_clock();
      TimeoutAction::ToDuration(Duration::from_secs(1))
    })
    .map_err(|e| anyhow::anyhow!("failed to insert clock timer into event loop: {e}"))?;

  while !state.exit {
    event_loop
      .dispatch(None, &mut state)
      .context("event loop dispatch failed")?;
    slint::platform::update_timers_and_animations();
    state.render_if_dirty()?;
    conn.flush().context("failed to flush Wayland connection")?;
  }

  Ok(())
}
