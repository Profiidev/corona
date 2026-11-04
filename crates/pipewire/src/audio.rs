use anyhow::{Context as _, Result, bail};

use crate::{
  Pipewire,
  command::{Command, Target},
  state::{AudioNode, NodeType},
  volume::to_linear,
};

pub struct PipewireAudio<'s>(pub(super) &'s Pipewire);

impl PipewireAudio<'_> {
  pub fn node(&self, id: u32) -> Option<AudioNode> {
    self.0.state.audio.nodes.get(&id).map(|node| node.clone())
  }

  fn stream(&self, id: u32) -> Result<AudioNode> {
    let node = self.node(id).context("No such audio node")?;
    if node.kind != NodeType::Stream {
      bail!("Not a stream");
    }
    Ok(node)
  }

  pub fn set_default(&self, id: u32) -> Result<()> {
    let node = self.node(id).context("No such audio node")?;
    if node.kind == NodeType::Stream {
      bail!("Streams have no default");
    }

    self.0.send(Command::SetDefault {
      kind: node.kind,
      name: node.name,
    })
  }

  pub fn set_target(&self, stream: u32, sink: u32) -> Result<()> {
    self.stream(stream)?;
    let sink = self.node(sink).context("No such audio node")?;
    if sink.kind != NodeType::Sink {
      bail!("Target is not a sink");
    }

    self.0.send(Command::SetTarget {
      node: stream,
      name: Some(sink.name),
    })
  }

  pub fn reset_target(&self, stream: u32) -> Result<()> {
    self.stream(stream)?;
    self.0.send(Command::SetTarget {
      node: stream,
      name: None,
    })
  }

  /// The same linear level on every channel of the node
  pub fn set_level(&self, id: u32, level: f32) -> Result<()> {
    let node = self.node(id).context("No such audio node")?;
    self.set_volumes(id, vec![level; node.volumes.len().max(1)])
  }

  pub fn set_volumes(&self, id: u32, volumes: Vec<f32>) -> Result<()> {
    if volumes.is_empty() {
      bail!("At least one volume is required");
    }
    if volumes.iter().any(|v| !v.is_finite()) {
      bail!("Volumes must be finite");
    }
    let max = to_linear(1.5);
    self.0.send(Command::SetVolumes {
      target: self.props_target(id)?,
      volumes: volumes.into_iter().map(|v| v.clamp(0., max)).collect(),
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
