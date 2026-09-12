use anyhow::{Context as _, Result};

use crate::integration::pipewire::{
  Pipewire,
  command::{Command, Target},
  state::{AudioNode, NodeType},
};

pub struct PipewireAudio<'s>(pub(super) &'s Pipewire);

impl PipewireAudio<'_> {
  pub fn list_sinks(&self) -> Vec<AudioNode> {
    self.list(NodeType::Sink)
  }

  pub fn list_sources(&self) -> Vec<AudioNode> {
    self.list(NodeType::Source)
  }

  pub fn list_streams(&self) -> Vec<AudioNode> {
    self.list(NodeType::Stream)
  }

  pub fn node(&self, id: u32) -> Option<AudioNode> {
    self.0.state.audio.nodes.get(&id).map(|node| node.clone())
  }

  pub fn set_volumes(&self, id: u32, volumes: Vec<f32>) -> Result<()> {
    self.0.send(Command::SetVolumes {
      target: self.target(id)?,
      volumes,
    })
  }

  pub fn set_mute(&self, id: u32, mute: bool) -> Result<()> {
    self.0.send(Command::SetMute {
      target: self.target(id)?,
      mute,
    })
  }

  fn list(&self, kind: NodeType) -> Vec<AudioNode> {
    let mut nodes: Vec<_> = self
      .0
      .state
      .audio
      .nodes
      .iter()
      .filter(|node| node.kind == kind)
      .map(|node| node.clone())
      .collect();

    nodes.sort_unstable_by_key(|node| node.id);
    nodes
  }

  fn target(&self, id: u32) -> Result<Target> {
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
