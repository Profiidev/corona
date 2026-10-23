use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use corona_compositor::CompositorExt;
use corona_ipc::{IpcCommand, IpcServer};
use corona_pipewire::{
  AudioNode, PipewireExt,
  volume::{to_linear, to_slider},
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::commands::volume::Action;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<ListNodes>()
    .register::<SetDefault>()
    .register::<StreamVolume>()
    .register::<MoveStream>()
    .register::<MuteFocused>();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Kind {
  Outputs,
  Inputs,
  Streams,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeInfo {
  pub id: u32,
  pub name: String,
  pub description: String,
  pub default: bool,
  /// 0..=1 as the sliders show it
  pub volume: f32,
  pub mute: bool,
}

fn nodes(kind: Kind, cx: &App) -> Vec<AudioNode> {
  let pipewire = cx.pipewire();
  match kind {
    Kind::Outputs => pipewire.list_sinks(cx),
    Kind::Inputs => pipewire.list_sources(cx),
    Kind::Streams => pipewire.list_streams(cx),
  }
  .to_vec()
}

/// The node with this id, `node.name` or description
fn find(kind: Kind, which: &str, cx: &App) -> Result<AudioNode> {
  nodes(kind, cx)
    .into_iter()
    .find(|n| {
      n.id.to_string() == which
        || n.name == which
        || n.description.eq_ignore_ascii_case(which)
        || n.app.iter().any(|a| a.eq_ignore_ascii_case(which))
    })
    .with_context(|| format!("no audio node {which}"))
}

pub struct ListNodes;

impl IpcCommand for ListNodes {
  const COMMAND: &'static str = "audio:list";

  type Payload = Kind;
  type Response = Vec<NodeInfo>;

  fn handle(kind: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let pipewire = cx.pipewire();
    let default = match kind {
      Kind::Outputs => pipewire.default_sink(cx).map(|n| n.id),
      Kind::Inputs => pipewire.default_source(cx).map(|n| n.id),
      Kind::Streams => None,
    };
    Ok(
      nodes(kind, cx)
        .into_iter()
        .map(|n| NodeInfo {
          default: Some(n.id) == default,
          volume: to_slider(n.volume()),
          mute: n.mute,
          id: n.id,
          name: n.name,
          description: n.description,
        })
        .collect(),
    )
  }
}

/// Makes an output or input the default
pub struct SetDefault;

impl IpcCommand for SetDefault {
  const COMMAND: &'static str = "audio:default";

  type Payload = (Kind, String);
  type Response = ();

  fn handle((kind, which): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    if kind == Kind::Streams {
      bail!("streams have no default");
    }
    let node = find(kind, &which, cx)?;
    cx.pipewire().audio().set_default(node.id)
  }
}

/// Volume or mute of one app's stream, by id or app name
pub struct StreamVolume;

impl IpcCommand for StreamVolume {
  const COMMAND: &'static str = "audio:stream";

  type Payload = (String, Action);
  type Response = ();

  fn handle((which, action): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let node = find(Kind::Streams, &which, cx)?;
    let audio = cx.pipewire().audio();
    let channels = node.volumes.len().max(1);
    match action {
      Action::Set(level) => {
        audio.set_volumes(node.id, vec![to_linear(level.clamp(0., 1.)); channels])
      }
      Action::Change(delta) => {
        let level = (to_slider(node.volume()) + delta).clamp(0., 1.);
        audio.set_volumes(node.id, vec![to_linear(level); channels])
      }
      Action::ToggleMute => audio.set_mute(node.id, !node.mute),
    }
  }
}

/// Plays a stream on another output
pub struct MoveStream;

impl IpcCommand for MoveStream {
  const COMMAND: &'static str = "audio:move";

  type Payload = (String, String);
  type Response = ();

  fn handle((stream, sink): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let stream = find(Kind::Streams, &stream, cx)?;
    let sink = find(Kind::Outputs, &sink, cx)?;
    cx.pipewire().audio().set_target(stream.id, sink.id)
  }
}

/// Toggles mute of the focused window's streams, matched by process
pub struct MuteFocused;

impl IpcCommand for MuteFocused {
  const COMMAND: &'static str = "audio:mute_focused";

  type Payload = ();
  /// Muted afterwards
  type Response = bool;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let window = cx
      .compositor()
      .active_window(cx)
      .cloned()
      .context("no window is focused")?;
    let class = window.class.to_lowercase();
    let streams: Vec<AudioNode> = nodes(Kind::Streams, cx)
      .into_iter()
      .filter(|s| {
        s.pid
          .is_some_and(|pid| pid as i32 == window.pid && window.pid > 0)
          || s.app.iter().any(|a| a.to_lowercase() == class)
      })
      .collect();
    if streams.is_empty() {
      bail!("{} plays no audio", window.class);
    }
    let mute = !streams.iter().all(|s| s.mute);
    let audio = cx.pipewire().audio();
    for stream in streams {
      audio.set_mute(stream.id, mute)?;
    }
    Ok(mute)
  }
}
