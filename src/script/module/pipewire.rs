use gpui_kit::{App, Entity};
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  integration::pipewire::{AudioNode, NodeType, Pipewire, PipewireExt},
  script::{
    host_fn::{Cx, Glob, HostReturn, Module, Named},
    module::{Subscribe, Subscriptions, watch},
  },
};
use corona_macros::{host_fn, named};

// The script's view of an `AudioNode`: `volume` added, device ids left out.
#[derive(Serialize, TS)]
#[ts(rename = "AudioNode")]
struct Node {
  id: u32,
  serial: u64,
  kind: NodeType,
  name: String,
  description: String,
  nickname: Option<String>,
  volume: f32,
  volumes: Vec<f32>,
  mute: bool,
  app: Vec<String>,
}

impl From<&AudioNode> for Node {
  fn from(node: &AudioNode) -> Self {
    Self {
      id: node.id,
      serial: node.serial,
      kind: node.kind,
      name: node.name.clone(),
      description: node.description.clone(),
      nickname: node.nickname.clone(),
      volume: node.volumes.first().copied().unwrap_or(0.),
      volumes: node.volumes.clone(),
      mute: node.mute,
      app: node.app.clone(),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Sinks,
  Sources,
  Streams,
  DefaultSink,
  DefaultSource,
  Targets,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Pipewire(value)
  }
}

/// A read that re-renders the script when `entity` changes, if the script read it.
fn read<W: 'static, R: HostReturn<M>, M: 'static>(
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  name: &'static str,
  update: Updates,
  entity: Entity<W>,
  read: impl Fn(&Pipewire, &App) -> R + 'static,
) -> Named<impl Fn(Cx) -> R + 'static> {
  subs.push(watch(reads, update.into(), entity));
  let reads = reads.clone();

  named!(name, move |cx: Cx| {
    reads.record(update.into());
    read(cx.pipewire(), &cx)
  })
}

fn nodes(nodes: &[AudioNode]) -> Vec<Node> {
  nodes.iter().map(Node::from).collect()
}

#[host_fn]
fn set_default(pipewire: Glob<Pipewire>, id: u32) -> anyhow::Result<()> {
  pipewire.audio().set_default(id)
}

#[host_fn]
fn set_target(pipewire: Glob<Pipewire>, stream: u32, sink: u32) -> anyhow::Result<()> {
  pipewire.audio().set_target(stream, sink)
}

#[host_fn]
fn reset_target(pipewire: Glob<Pipewire>, stream: u32) -> anyhow::Result<()> {
  pipewire.audio().reset_target(stream)
}

#[host_fn]
fn set_volume(pipewire: Glob<Pipewire>, id: u32, volume: f32) -> anyhow::Result<()> {
  let channels = pipewire
    .audio()
    .node(id)
    .map_or(1, |node| node.volumes.len().max(1));

  pipewire.audio().set_volumes(id, vec![volume; channels])
}

#[host_fn]
fn set_mute(pipewire: Glob<Pipewire>, id: u32, mute: bool) -> anyhow::Result<()> {
  pipewire.audio().set_mute(id, mute)
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let pipewire = cx.pipewire();

  Module::new("corona/pipewire")
    .func(read(
      reads,
      subs,
      "listSinks",
      Updates::Sinks,
      pipewire.sinks.clone(),
      |pipewire, cx| nodes(pipewire.list_sinks(cx)),
    ))
    .func(read(
      reads,
      subs,
      "listSources",
      Updates::Sources,
      pipewire.sources.clone(),
      |pipewire, cx| nodes(pipewire.list_sources(cx)),
    ))
    .func(read(
      reads,
      subs,
      "listStreams",
      Updates::Streams,
      pipewire.streams.clone(),
      |pipewire, cx| nodes(pipewire.list_streams(cx)),
    ))
    .func(read(
      reads,
      subs,
      "defaultSink",
      Updates::DefaultSink,
      pipewire.default_sink.clone(),
      |pipewire, cx| pipewire.default_sink(cx).map(Node::from),
    ))
    .func(read(
      reads,
      subs,
      "defaultSource",
      Updates::DefaultSource,
      pipewire.default_source.clone(),
      |pipewire, cx| pipewire.default_source(cx).map(Node::from),
    ))
    .func({
      subs.push(watch(
        reads,
        Updates::Targets.into(),
        pipewire.targets.clone(),
      ));
      let reads = reads.clone();

      named!(
        "target",
        /// The id of the sink a stream is routed through, or null to follow the default.
        move |cx: Cx, pipewire: Glob<Pipewire>, stream: u32| {
          reads.record(Updates::Targets.into());
          pipewire.target(stream, &cx)
        }
      )
    })
    .func(set_default)
    .func(set_target)
    .func(reset_target)
    .func(set_volume)
    .func(set_mute)
    .into()
}
