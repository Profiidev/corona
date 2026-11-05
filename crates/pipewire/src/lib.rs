use std::{thread, time::Duration};

use anyhow::Result;
use corona_js::ScriptExt;
use corona_utils::Buses;
use gpui_kit::App;
use pipewire::{context::ContextRc, core::CoreRc, main_loop::MainLoopRc, registry::RegistryRc};

use crate::{event::AudioEvent, listener::Handles, state::PipewireState};

mod api;
mod audio;
mod capture;
pub mod cli;
mod command;
pub mod commands;
mod event;
mod launcher;
mod listener;
pub mod script;
pub mod settings;
mod state;
#[cfg(test)]
mod testing;
pub mod ui;
pub mod volume;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

pub use api::Pipewire;
pub use audio::PipewireAudio;
pub use capture::{Capture, CaptureAccess, CaptureKind};
pub use state::{AudioNode, NodeType};

corona_utils::global_ext!(PipewireExt, pipewire, Pipewire);

/// Starts the PipeWire thread, registers the audio and privacy UI, commands and script module
pub fn init(cx: &mut App, _: &Buses) -> Result<()> {
  start(cx)?;
  ui::register(cx);
  commands::register(cx);
  launcher::register(cx);
  settings::register(cx);
  register_script(cx.script_mut());
  Ok(())
}

/// Registers this integration's script modules, for the shell and the CLI
pub fn register_script(registry: &mut corona_js::ScriptRegistry) {
  registry.register::<script::PipewireModule>();
}

fn start(cx: &mut App) -> Result<()> {
  let (event_tx, event_rx) = flume::unbounded();
  let (command_tx, state) = spawn(event_tx)?;

  let pipewire = Pipewire::new(cx, command_tx, state, event_rx);
  cx.set_global(pipewire);

  Ok(())
}

fn spawn(
  event_tx: flume::Sender<AudioEvent>,
) -> Result<(pipewire::channel::Sender<command::Command>, PipewireState)> {
  let (init_tx, init_rx) = flume::bounded::<Result<pipewire::channel::Sender<command::Command>>>(1);

  let state = PipewireState::new();
  let thread_state = state.clone();

  thread::spawn(move || {
    let mainloop = match MainLoopRc::new(None) {
      Ok(mainloop) => mainloop,
      Err(e) => {
        init_tx
          .send(Err(e.into()))
          .expect("Failed to send error from pipewire init");
        return;
      }
    };

    let handles = Handles::default();

    // lives across reconnects; commands sent while disconnected stay queued and run at the start of the next
    // mainloop.run(), before the new registry globals arrive, so they find empty handles and fail cleanly
    let (command_tx, command_rx) = pipewire::channel::channel();
    let _command_rx = command_rx.attach(mainloop.loop_(), {
      let handles = handles.clone();
      move |command: command::Command| command.execute(&handles)
    });

    init_tx
      .send(Ok(command_tx))
      .expect("Failed to send success from pipewire init");

    let mut attempt = 0;
    // the app is gone once nobody receives events anymore
    while !event_tx.is_disconnected() {
      match connect(&mainloop) {
        Ok((core, registry)) => {
          attempt = 0;
          let _core_listener = core
            .add_listener_local()
            // an error on the core object (id 0) is a broken connection, e.g. EPIPE after a restart
            .error({
              let mainloop = mainloop.clone();
              move |id, _, res, message| {
                if id == pipewire::core::PW_ID_CORE {
                  tracing::warn!("PipeWire connection lost ({res}): {message}");
                  mainloop.quit();
                }
              }
            })
            .register();
          let _listener = registry
            .add_listener_local()
            .global(listener::global_listener(
              registry.clone(),
              handles.clone(),
              thread_state.clone(),
              event_tx.clone(),
            ))
            .global_remove(listener::global_remove_listener(
              handles.clone(),
              thread_state.clone(),
              event_tx.clone(),
            ))
            .register();

          mainloop.run();

          // drop proxies before the registry and core they belong to
          handles.clear();
          thread_state.reset(&event_tx);
        }
        Err(e) => tracing::debug!("PipeWire connect failed: {e}"),
      }
      thread::sleep(backoff(attempt));
      attempt += 1;
    }
  });

  Ok((init_rx.recv()??, state))
}

fn connect(mainloop: &MainLoopRc) -> Result<(CoreRc, RegistryRc)> {
  let context = ContextRc::new(mainloop, None)?;
  let core = context.connect_rc(None)?;
  let registry = core.get_registry_rc()?;

  Ok((core, registry))
}

/// Delay before reconnect attempt `attempt` (0-based): 250ms doubling up to 5s
fn backoff(attempt: u32) -> Duration {
  Duration::from_millis(250u64 << attempt.min(5)).min(Duration::from_secs(5))
}

#[cfg(test)]
mod backoff_tests {
  use super::*;

  #[test]
  fn doubles_then_caps() {
    let ms: Vec<_> = (0..8).map(|a| backoff(a).as_millis()).collect();
    assert_eq!(ms, [250, 500, 1000, 2000, 4000, 5000, 5000, 5000]);
    assert_eq!(backoff(u32::MAX), Duration::from_secs(5));
  }
}
