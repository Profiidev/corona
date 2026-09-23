use anyhow::{Context as _, Result};

use crate::integration::pipewire::{
  Pipewire,
  command::{Command, Target},
  state::AudioNode,
};

pub struct PipewireAudio<'s>(pub(super) &'s Pipewire);

impl PipewireAudio<'_> {
  pub fn node(&self, id: u32) -> Option<AudioNode> {
    self.0.state.audio.nodes.get(&id).map(|node| node.clone())
  }

  pub fn set_default(&self, id: u32) -> Result<()> {
    let node = self.node(id).context("No such audio node")?;

    self.0.send(Command::SetDefault {
      kind: node.kind,
      name: node.name,
    })
  }

  pub fn set_target(&self, stream: u32, sink: u32) -> Result<()> {
    let sink = self.node(sink).context("No such audio node")?;

    self.0.send(Command::SetTarget {
      node: stream,
      name: Some(sink.name),
    })
  }

  pub fn reset_target(&self, stream: u32) -> Result<()> {
    self.0.send(Command::SetTarget {
      node: stream,
      name: None,
    })
  }

  pub fn set_volumes(&self, id: u32, volumes: Vec<f32>) -> Result<()> {
    self.0.send(Command::SetVolumes {
      target: self.props_target(id)?,
      volumes,
    })
  }

  pub fn set_mute(&self, id: u32, mute: bool) -> Result<()> {
    self.0.send(Command::SetMute {
      target: self.props_target(id)?,
      mute,
    })
  }

  fn props_target(&self, id: u32) -> Result<Target> {
    let node = self.node(id).context("No such audio node")?;

    Ok(match (node.device, node.profile_device) {
      (Some(device), Some(profile_device)) => Target::Route {
        node: id,
        device,
        profile_device,
      },
      _ => Target::Node(id),
    })
  }
}
