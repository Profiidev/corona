use std::thread;

use anyhow::Result;
use gpui_kit::App;
use pipewire::{context::ContextRc, main_loop::MainLoopRc, registry::RegistryRc};

use crate::integration::pipewire::state::PipewireState;

mod api;
mod listener;
mod state;

pub use api::Pipewire;

pub fn init(cx: &mut App) -> Result<()> {
  let (init_tx, init_rx) = flume::bounded(1);

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

    let state = PipewireState::new();
    let (pipewire, _rx) = Pipewire::init(&mainloop, state.clone());

    init_tx
      .send(Ok(pipewire))
      .expect("Failed to send success from pipewire init");

    let _listener = registry
      .add_listener_local()
      .global(listener::global_listener(registry.clone(), state))
      .register();

    mainloop.run();
  });

  let pipewire = init_rx.recv()??;

  thread::sleep(std::time::Duration::from_millis(100));
  dbg!(pipewire.list_audio_sinks());

  cx.set_global(pipewire);

  Ok(())
}

fn init_loop() -> Result<(MainLoopRc, RegistryRc)> {
  let mainloop = MainLoopRc::new(None)?;
  let context = ContextRc::new(&mainloop, None)?;
  let core = context.connect_rc(None)?;
  let registry = core.get_registry_rc()?;

  Ok((mainloop, registry))
}
