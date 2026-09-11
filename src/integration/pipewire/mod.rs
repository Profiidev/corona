use std::thread;

use anyhow::Result;
use gpui_kit::App;
use pipewire::{context::ContextRc, main_loop::MainLoopRc, registry::RegistryRc};

use crate::integration::pipewire::{listener::Handles, state::PipewireState};

mod api;
mod audio;
mod command;
mod event;
mod listener;
mod state;

pub use api::Pipewire;
pub use event::PipewireEvent;
pub use state::AudioSink;

pub fn init(cx: &mut App) -> Result<()> {
  let (event_tx, event_rx) = flume::unbounded();
  let (command_tx, state) = spawn(event_tx)?;

  let pipewire = Pipewire::new(cx, command_tx, state, event_rx);
  cx.set_global(pipewire);

  Ok(())
}

fn spawn(
  event_tx: flume::Sender<PipewireEvent>,
) -> Result<(pipewire::channel::Sender<command::Command>, PipewireState)> {
  let (init_tx, init_rx) = flume::bounded(1);

  let state = PipewireState::new();
  let thread_state = state.clone();

  thread::spawn(move || {
    let (mainloop, registry) = match init_loop() {
      Ok((mainloop, registry)) => (mainloop, registry),
      Err(e) => {
        init_tx
          .send(Err(e))
          .expect("Failed to send error from pipewire init");
        return;
      }
    };

    let handles = Handles::default();

    let (command_tx, command_rx) = pipewire::channel::channel();
    let _command_rx = command_rx.attach(mainloop.loop_(), {
      let handles = handles.clone();
      move |command: command::Command| command.execute(&handles)
    });

    init_tx
      .send(Ok(command_tx))
      .expect("Failed to send success from pipewire init");

    let _listener = registry
      .add_listener_local()
      .global(listener::global_listener(
        registry.clone(),
        handles.clone(),
        thread_state.clone(),
        event_tx.clone(),
      ))
      .global_remove(listener::global_remove_listener(
        handles,
        thread_state,
        event_tx,
      ))
      .register();

    mainloop.run();
  });

  Ok((init_rx.recv()??, state))
}

fn init_loop() -> Result<(MainLoopRc, RegistryRc)> {
  let mainloop = MainLoopRc::new(None)?;
  let context = ContextRc::new(&mainloop, None)?;
  let core = context.connect_rc(None)?;
  let registry = core.get_registry_rc()?;

  Ok((mainloop, registry))
}
