use std::{collections::HashMap, time::SystemTime};

use anyhow::{Result, bail};
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global};

use crate::{
  audio::PipewireAudio,
  capture::{Capture, CaptureAccess, CaptureKind, record},
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
  pub captures: Entity<Vec<Capture>>,
  pub capture_log: Entity<Vec<CaptureAccess>>,
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
      captures: cx.new(|_| state.captures.list()),
      capture_log: cx.new(|_| {
        let mut log = Vec::new();
        record(&mut log, &state.captures.list(), SystemTime::now());
        log
      }),
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
            AudioEvent::Captures(captures) => {
              pipewire.capture_log.update(cx, |log, cx| {
                if record(log, &captures, SystemTime::now()) {
                  cx.notify();
                }
              });
              pipewire.captures.write_changed(cx, captures);
            }
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

  pub fn capture_log<'c>(&self, cx: &'c App) -> &'c [CaptureAccess] {
    self.capture_log.read(cx)
  }

  pub fn list_captures<'c>(&self, cx: &'c App) -> &'c [Capture] {
    self.captures.read(cx)
  }

  pub fn is_capturing(&self, kind: CaptureKind, cx: &App) -> bool {
    self
      .list_captures(cx)
      .iter()
      .any(|c| c.kind == kind && c.active)
  }

  pub fn capturing(&self, kind: CaptureKind, cx: &App) -> Vec<String> {
    let mut names: Vec<String> = self
      .list_captures(cx)
      .iter()
      .filter(|c| c.kind == kind && c.active && !c.name.is_empty())
      .map(|c| c.name.clone())
      .collect();
    names.sort();
    names.dedup();
    names
  }

  pub fn target(&self, stream: u32, cx: &App) -> Option<u32> {
    self.targets.read(cx).get(&stream).copied()
  }
}
