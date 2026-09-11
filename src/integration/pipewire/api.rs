use anyhow::{Result, bail};
use gpui_kit::{App, AppContext, Entity, Global};

use crate::integration::pipewire::{
  audio::PipewireAudio,
  command::Command,
  event::{PipewireEvent, PipewireEventEmitter},
  state::PipewireState,
};

pub struct Pipewire {
  commands: pipewire::channel::Sender<Command>,
  pub(super) state: PipewireState,
  events: Entity<PipewireEventEmitter>,
}

impl Global for Pipewire {}

impl Pipewire {
  pub(super) fn new(
    cx: &mut App,
    commands: pipewire::channel::Sender<Command>,
    state: PipewireState,
    rx: flume::Receiver<PipewireEvent>,
  ) -> Self {
    let events = cx.new(|cx| {
      cx.spawn(async move |this, cx| {
        while let Ok(event) = rx.recv_async().await {
          let _ = this.update(cx, |_, cx| cx.emit(event));
        }
      })
      .detach();

      PipewireEventEmitter
    });

    Self {
      commands,
      state,
      events,
    }
  }

  pub(super) fn send(&self, command: Command) -> Result<()> {
    if self.commands.send(command).is_err() {
      bail!("Failed to send command to pipewire");
    }
    Ok(())
  }

  pub fn emitter(&self) -> &Entity<PipewireEventEmitter> {
    &self.events
  }

  pub fn audio(&self) -> PipewireAudio<'_> {
    PipewireAudio(&self)
  }
}
