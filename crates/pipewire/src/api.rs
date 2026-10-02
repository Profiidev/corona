use std::collections::HashMap;

use anyhow::{Result, bail};
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global};

use crate::{
  audio::PipewireAudio,
  command::Command,
  event::AudioEvent,
  state::{AudioNode, NodeType, PipewireState},
};

pub struct Pipewire {
  commands: pipewire::channel::Sender<Command>,
  pub(super) state: PipewireState,
  pub sinks: Entity<Vec<AudioNode>>,
  pub sources: Entity<Vec<AudioNode>>,
  pub streams: Entity<Vec<AudioNode>>,
  pub default_sink: Entity<Option<AudioNode>>,
  pub default_source: Entity<Option<AudioNode>>,
  pub targets: Entity<HashMap<u32, u32>>,
}

impl Global for Pipewire {}

impl Pipewire {
  pub(super) fn new(
    cx: &mut App,
    commands: pipewire::channel::Sender<Command>,
    state: PipewireState,
    rx: flume::Receiver<AudioEvent>,
  ) -> Self {
    let audio = &state.audio;
    let pipewire = Self {
      sinks: cx.new(|_| audio.list(NodeType::Sink)),
      sources: cx.new(|_| audio.list(NodeType::Source)),
      streams: cx.new(|_| audio.list(NodeType::Stream)),
      default_sink: cx.new(|_| audio.default(NodeType::Sink)),
      default_source: cx.new(|_| audio.default(NodeType::Source)),
      targets: cx.new(|_| audio.resolved_targets()),
      commands,
      state,
    };

    cx.spawn(async move |cx| {
      while let Ok(event) = rx.recv_async().await {
        cx.update(|cx| {
          cx.update_global::<Pipewire, _>(|pipewire, cx| match event {
            AudioEvent::Nodes(NodeType::Sink, nodes) => pipewire.sinks.write_changed(cx, nodes),
            AudioEvent::Nodes(NodeType::Source, nodes) => pipewire.sources.write_changed(cx, nodes),
            AudioEvent::Nodes(NodeType::Stream, nodes) => pipewire.streams.write_changed(cx, nodes),
            AudioEvent::DefaultSink(node) => pipewire.default_sink.write_changed(cx, node),
            AudioEvent::DefaultSource(node) => pipewire.default_source.write_changed(cx, node),
            AudioEvent::Targets(targets) => pipewire.targets.write_changed(cx, targets),
          });
        });
      }
    })
    .detach();

    pipewire
  }

  pub(super) fn send(&self, command: Command) -> Result<()> {
    if self.commands.send(command).is_err() {
      bail!("Failed to send command to pipewire");
    }
    Ok(())
  }

  pub fn audio(&self) -> PipewireAudio<'_> {
    PipewireAudio(self)
  }

  pub fn list_sinks<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.sinks.read(cx)
  }

  pub fn list_sources<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.sources.read(cx)
  }

  pub fn list_streams<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.streams.read(cx)
  }

  pub fn default_sink<'c>(&self, cx: &'c App) -> Option<&'c AudioNode> {
    self.default_sink.read(cx).as_ref()
  }

  pub fn default_source<'c>(&self, cx: &'c App) -> Option<&'c AudioNode> {
    self.default_source.read(cx).as_ref()
  }

  pub fn target(&self, stream: u32, cx: &App) -> Option<u32> {
    self.targets.read(cx).get(&stream).copied()
  }
}
