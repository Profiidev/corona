mod app;
mod egl;
mod platform;
mod window;

use anyhow::{Context, Result};
use app::AppState;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::Connection;
use wayland_client::globals::registry_queue_init;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let conn = Connection::connect_to_env()
        .context("no Wayland compositor found (is Hyprland running?)")?;
    let (globals, event_queue) =
        registry_queue_init::<AppState>(&conn).context("failed to enumerate Wayland globals")?;
    let qh = event_queue.handle();

    let mut state = AppState::new(&conn, &qh, &globals)?;

    let mut event_loop: EventLoop<AppState> =
        EventLoop::try_new().context("failed to create event loop")?;
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("failed to insert Wayland source into event loop: {e}"))?;

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
