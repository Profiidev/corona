use anyhow::Result;

use crate::integration::pipewire::{Pipewire, command::Command, state::AudioSink};

pub struct PipewireAudio<'s>(pub(super) &'s Pipewire);

impl PipewireAudio<'_> {
  pub fn list_sinks(&self) -> Vec<AudioSink> {
    self
      .0
      .state
      .audio
      .sinks
      .iter()
      .map(|entry| entry.clone())
      .collect()
  }

  pub fn sink(&self, id: u32) -> Option<AudioSink> {
    self.0.state.audio.sinks.get(&id).map(|entry| entry.clone())
  }

  pub fn set_sink_volumes(&self, id: u32, volumes: Vec<f32>) -> Result<()> {
    self.0.send(Command::SetVolumes { id, volumes })
  }

  pub fn set_sink_mute(&self, id: u32, mute: bool) -> Result<()> {
    self.0.send(Command::SetMute { id, mute })
  }
}
