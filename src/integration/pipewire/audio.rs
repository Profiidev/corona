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

  pub fn default_sink(&self) -> Option<AudioNode> {
    self.default(NodeType::Sink)
  }

  pub fn default_source(&self) -> Option<AudioNode> {
    self.default(NodeType::Source)
  }

  pub fn set_default(&self, id: u32) -> Result<()> {
    let node = self.node(id).context("No such audio node")?;

    self.0.send(Command::SetDefault {
      kind: node.kind,
      name: node.name,
    })
  }

  pub fn target(&self, stream: u32) -> Option<AudioNode> {
    let target = self.0.state.audio.targets.get(&stream)?;

    let serial = target.parse().ok();
    self.find(|node| Some(node.serial) == serial || node.name == *target)
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

  fn default(&self, kind: NodeType) -> Option<AudioNode> {
    let name = self.0.state.audio.defaults.get(&kind)?;
    self.by_name(&name)
  }

  fn by_name(&self, name: &str) -> Option<AudioNode> {
    self.find(|node| node.name == name)
  }

  fn find(&self, matches: impl Fn(&AudioNode) -> bool) -> Option<AudioNode> {
    self
      .0
      .state
      .audio
      .nodes
      .iter()
      .find(|node| matches(node))
      .map(|node| node.clone())
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
