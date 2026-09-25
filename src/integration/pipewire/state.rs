use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use pipewire::spa::{
  pod::{Object, Value, ValueArray},
  sys,
  utils::dict::DictRef,
};
use serde::Serialize;
use ts_rs::TS;

use crate::integration::pipewire::event::AudioEvent;

#[derive(Clone)]
pub struct PipewireState {
  pub audio: AudioState,
}

impl PipewireState {
  pub fn new() -> Self {
    Self {
      audio: AudioState {
        nodes: Arc::new(DashMap::new()),
        defaults: Arc::new(DashMap::new()),
        targets: Arc::new(DashMap::new()),
      },
    }
  }
}

#[derive(Clone)]
pub struct AudioState {
  pub nodes: Arc<DashMap<u32, AudioNode>>,
  pub defaults: Arc<DashMap<NodeType, String>>,
  pub targets: Arc<DashMap<u32, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "NodeKind")]
pub enum NodeType {
  Sink,
  Source,
  Stream,
}

impl FromStr for NodeType {
  type Err = anyhow::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Audio/Sink" => Ok(NodeType::Sink),
      "Audio/Source" => Ok(NodeType::Source),
      "Stream/Output/Audio" => Ok(NodeType::Stream),
      _ => Err(anyhow!("Invalid node type: {}", s)),
    }
  }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioNode {
  pub id: u32,
  pub serial: u64,
  pub kind: NodeType,
  pub name: String,
  pub description: String,
  pub nickname: Option<String>,
  pub device: Option<u32>,
  pub profile_device: Option<i32>,
  pub volumes: Vec<f32>,
  pub mute: bool,
  pub app: Vec<String>,
}

fn app(props: &DictRef) -> Vec<String> {
  [
    "application.icon-name",
    "application.id",
    "application.name",
    "application.process.binary",
  ]
  .into_iter()
  .filter_map(|key| props.get(key))
  .map(|value| value.to_string())
  .collect()
}

fn label(props: &DictRef) -> Option<&str> {
  ["node.description", "media.name", "application.name"]
    .into_iter()
    .find_map(|key| props.get(key))
}

impl AudioNode {
  pub fn new(id: u32, kind: NodeType, props: &DictRef) -> Option<Self> {
    let name = props.get("node.name")?.to_string();

    Some(Self {
      id,
      serial: props
        .get("object.serial")
        .and_then(|serial| serial.parse().ok())
        .unwrap_or_default(),
      kind,
      description: label(props).unwrap_or(&name).to_string(),
      name,
      nickname: props.get("node.nick").map(|nick| nick.to_string()),
      device: props.get("device.id").and_then(|id| id.parse().ok()),
      profile_device: None,
      mute: false,
      volumes: vec![0.0],
      app: app(props),
    })
  }

  fn update_props(&mut self, props: &DictRef) -> bool {
    let mut changed = false;
    changed |= update(&mut self.serial, props.get("object.serial"));
    changed |= update(&mut self.name, props.get("node.name"));
    changed |= update(&mut self.description, label(props));
    changed |= update_optional(&mut self.nickname, props.get("node.nick"));
    changed |= update_optional(&mut self.device, props.get("device.id"));
    changed |= update_optional(&mut self.profile_device, props.get("card.profile.device"));
    let app = app(props);
    if !app.is_empty() && app != self.app {
      self.app = app;
      changed = true;
    }
    changed
  }

  fn update_params(&mut self, obj: Object) -> bool {
    let mut changed = false;
    for prop in obj.properties {
      match (prop.key, prop.value) {
        (sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(volumes))) => {
          self.volumes = volumes;
          changed = true;
        }
        (sys::SPA_PROP_mute, Value::Bool(mute)) => {
          self.mute = mute;
          changed = true;
        }
        _ => (),
      }
    }
    changed
  }
}

impl AudioState {
  pub fn update_params(&self, id: u32, obj: Object, events: &flume::Sender<AudioEvent>) {
    self.changed(id, events, |node| node.update_params(obj));
  }

  pub fn update_props(&self, id: u32, props: &DictRef, events: &flume::Sender<AudioEvent>) {
    self.changed(id, events, |node| node.update_props(props));
  }

  pub fn set_default(
    &self,
    kind: NodeType,
    name: Option<String>,
    events: &flume::Sender<AudioEvent>,
  ) {
    let changed = match name {
      Some(name) => self.defaults.insert(kind, name.clone()) != Some(name),
      None => self.defaults.remove(&kind).is_some(),
    };

    if changed {
      let _ = events.send(self.default_event(kind));
    }
  }

  pub fn set_target(&self, id: u32, name: Option<String>, events: &flume::Sender<AudioEvent>) {
    let changed = match name {
      Some(name) => self.targets.insert(id, name.clone()) != Some(name),
      None => self.targets.remove(&id).is_some(),
    };

    if changed {
      let _ = events.send(AudioEvent::Targets(self.resolved_targets()));
    }
  }

  pub fn list(&self, kind: NodeType) -> Vec<AudioNode> {
    let mut nodes: Vec<AudioNode> = self
      .nodes
      .iter()
      .filter(|node| node.kind == kind)
      .map(|node| node.clone())
      .collect();
    nodes.sort_unstable_by_key(|node| node.id);
    nodes
  }

  pub fn default(&self, kind: NodeType) -> Option<AudioNode> {
    let name = self.defaults.get(&kind)?;
    self
      .nodes
      .iter()
      .find(|node| node.kind == kind && node.name == *name)
      .map(|node| node.clone())
  }

  pub fn resolved_targets(&self) -> HashMap<u32, u32> {
    self
      .targets
      .iter()
      .filter_map(|pin| {
        let serial = pin.value().parse().ok();
        let sink = self.nodes.iter().find(|node| {
          node.kind == NodeType::Sink && (Some(node.serial) == serial || node.name == *pin.value())
        })?;
        Some((*pin.key(), sink.id))
      })
      .collect()
  }

  pub fn node_events(&self, kind: NodeType) -> Vec<AudioEvent> {
    let mut events = vec![AudioEvent::Nodes(kind, self.list(kind))];
    match kind {
      NodeType::Sink => {
        events.push(self.default_event(kind));
        events.push(AudioEvent::Targets(self.resolved_targets()));
      }
      NodeType::Source => events.push(self.default_event(kind)),
      NodeType::Stream => events.push(AudioEvent::Targets(self.resolved_targets())),
    }
    events
  }

  fn default_event(&self, kind: NodeType) -> AudioEvent {
    let node = self.default(kind);
    match kind {
      NodeType::Source => AudioEvent::DefaultSource(node),
      _ => AudioEvent::DefaultSink(node),
    }
  }

  fn changed(
    &self,
    id: u32,
    events: &flume::Sender<AudioEvent>,
    update: impl FnOnce(&mut AudioNode) -> bool,
  ) {
    let Some(mut node) = self.nodes.get_mut(&id) else {
      return;
    };

    if !update(&mut node) {
      return;
    }

    let kind = node.kind;
    drop(node);

    for event in self.node_events(kind) {
      let _ = events.send(event);
    }
  }
}

fn update<T: FromStr + PartialEq>(field: &mut T, value: Option<&str>) -> bool {
  let Some(value) = value.and_then(|value| value.parse().ok()) else {
    return false;
  };

  if *field == value {
    return false;
  }

  *field = value;
  true
}

fn update_optional<T: FromStr + PartialEq>(field: &mut Option<T>, value: Option<&str>) -> bool {
  let Some(value) = value.and_then(|value| value.parse().ok()) else {
    return false;
  };

  if field.as_ref() == Some(&value) {
    return false;
  }

  *field = Some(value);
  true
}

#[cfg(test)]
mod tests {
  use pipewire::spa::{static_dict, utils::dict::StaticDict};

  use super::*;

  fn sink() -> AudioNode {
    static PROPS: StaticDict = static_dict! {
      "object.serial" => "62",
      "node.name" => "alsa_output.speaker",
      "node.description" => "Speaker",
      "node.nick" => "Built-in",
      "device.id" => "47",
    };

    AudioNode::new(54, NodeType::Sink, &PROPS).expect("the registry props are complete")
  }

  #[test]
  fn info_fills_in_what_the_registry_left_out() {
    static INFO: StaticDict = static_dict! {
      "node.name" => "alsa_output.speaker",
      "node.description" => "Speaker",
      "card.profile.device" => "0",
    };

    let mut sink = sink();
    assert_eq!(sink.profile_device, None);

    assert!(sink.update_props(&INFO));
    assert_eq!(sink.profile_device, Some(0));
    // Reported again unchanged, so no event is worth emitting.
    assert!(!sink.update_props(&INFO));
  }

  #[test]
  fn a_rename_reaches_the_sink() {
    static RENAMED: StaticDict = static_dict! {
      "node.description" => "Speaker (Dock)",
      "node.nick" => "Dock",
    };

    let mut sink = sink();

    assert!(sink.update_props(&RENAMED));
    assert_eq!(sink.description, "Speaker (Dock)");
    assert_eq!(sink.nickname.as_deref(), Some("Dock"));
    assert_eq!(sink.name, "alsa_output.speaker");
  }

  #[test]
  fn a_stream_is_named_by_its_media_and_written_on_the_node() {
    static STREAM: StaticDict = static_dict! {
      "object.serial" => "202",
      "node.name" => "spotify",
      "application.name" => "spotify",
      "application.process.binary" => ".spotify-wrapped",
      "media.name" => "Spotify",
      "media.class" => "Stream/Output/Audio",
    };

    let stream =
      AudioNode::new(110, NodeType::Stream, &STREAM).expect("a stream needs only a node.name");

    assert_eq!(stream.description, "Spotify");
    // The binary is decorated and the name is not, so both are worth keeping.
    assert_eq!(stream.app, ["spotify", ".spotify-wrapped"]);
    assert_eq!(stream.serial, 202);
    // No card, so nothing to route through.
    assert_eq!(stream.device, None);
    assert_eq!(stream.profile_device, None);
  }

  /// The trap that cost a session: a narrower info event used to clear
  /// `profile_device`, and the next volume write silently fell back to the node.
  #[test]
  fn a_narrower_info_event_clears_nothing() {
    static FULL: StaticDict = static_dict! {
      "node.description" => "Speaker",
      "node.nick" => "Built-in",
      "card.profile.device" => "0",
    };
    static NARROW: StaticDict = static_dict! {
      "node.name" => "alsa_output.speaker",
    };

    let mut sink = sink();
    sink.update_props(&FULL);

    assert!(!sink.update_props(&NARROW));
    assert_eq!(sink.profile_device, Some(0));
    assert_eq!(sink.description, "Speaker");
    assert_eq!(sink.nickname.as_deref(), Some("Built-in"));
    assert_eq!(sink.device, Some(47));
  }

  #[test]
  fn a_default_is_reported_once_per_change() {
    let (tx, rx) = flume::unbounded();
    let audio = PipewireState::new().audio;

    audio.set_default(NodeType::Sink, Some("speaker".into()), &tx);
    // Wireplumber republishes the same value freely; that is not a change.
    audio.set_default(NodeType::Sink, Some("speaker".into()), &tx);
    audio.set_default(NodeType::Sink, Some("headset".into()), &tx);
    audio.set_default(NodeType::Sink, None, &tx);

    assert_eq!(rx.len(), 3);
    assert!(audio.defaults.is_empty());
  }

  fn stream(id: u32, serial: u64) -> AudioNode {
    static PROPS: StaticDict = static_dict! {
      "node.name" => "spotify",
      "media.name" => "Spotify",
    };

    let mut node = AudioNode::new(id, NodeType::Stream, &PROPS).expect("a stream needs a name");
    node.serial = serial;
    node
  }

  /// A pin is stored as whatever wireplumber wrote — a name or a serial — so
  /// both have to resolve to the same sink id.
  #[test]
  fn a_pin_resolves_by_either_name_or_serial() {
    let (tx, _rx) = flume::unbounded();
    let audio = PipewireState::new().audio;
    audio.nodes.insert(54, sink());
    audio.nodes.insert(110, stream(110, 202));
    audio.nodes.insert(111, stream(111, 203));

    audio.set_target(110, Some("alsa_output.speaker".into()), &tx);
    audio.set_target(111, Some("62".into()), &tx);

    let targets = audio.resolved_targets();
    assert_eq!(targets.get(&110), Some(&54));
    assert_eq!(targets.get(&111), Some(&54));
  }

  /// The removal path projects *after* the node is gone, so nothing may still
  /// name it — not the list, not the default, not a stream's pin.
  #[test]
  fn removing_a_sink_clears_the_default_and_every_pin_naming_it() {
    let (tx, _rx) = flume::unbounded();
    let audio = PipewireState::new().audio;
    audio.nodes.insert(54, sink());
    audio.nodes.insert(110, stream(110, 202));
    audio.set_default(NodeType::Sink, Some("alsa_output.speaker".into()), &tx);
    audio.set_target(110, Some("alsa_output.speaker".into()), &tx);

    assert!(audio.default(NodeType::Sink).is_some());
    assert_eq!(audio.resolved_targets().get(&110), Some(&54));

    audio.nodes.remove(&54);

    assert!(audio.list(NodeType::Sink).is_empty());
    // The name is still pinned; it just no longer names anything that exists.
    assert!(audio.default(NodeType::Sink).is_none());
    assert!(audio.resolved_targets().is_empty());
  }

  /// One changed sink moves three slices, and must not claim to move a source.
  #[test]
  fn a_sink_change_projects_only_the_slices_it_can_move() {
    let audio = PipewireState::new().audio;
    audio.nodes.insert(54, sink());

    let events = audio.node_events(NodeType::Sink);
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], AudioEvent::Nodes(NodeType::Sink, _)));
    assert!(matches!(events[1], AudioEvent::DefaultSink(_)));
    assert!(matches!(events[2], AudioEvent::Targets(_)));

    let events = audio.node_events(NodeType::Source);
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], AudioEvent::Nodes(NodeType::Source, _)));
    assert!(matches!(events[1], AudioEvent::DefaultSource(_)));
  }

  #[test]
  fn a_reset_drops_the_pin() {
    let (tx, rx) = flume::unbounded();
    let audio = PipewireState::new().audio;

    audio.set_target(110, Some("alsa_output.hdmi".into()), &tx);
    assert_eq!(
      audio.targets.get(&110).map(|name| name.clone()),
      Some("alsa_output.hdmi".to_string())
    );

    audio.set_target(110, None, &tx);
    assert!(audio.targets.is_empty());
    // Nothing was pinned, so nothing changed.
    audio.set_target(110, None, &tx);

    assert_eq!(rx.len(), 2);
  }
}
