use std::{str::FromStr, sync::Arc};

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use pipewire::spa::{
  pod::{Object, Value, ValueArray},
  sys,
  utils::dict::DictRef,
};

use crate::integration::pipewire::event::PipewireEvent;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug)]
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
  pub fn update_params(&self, id: u32, obj: Object, events: &flume::Sender<PipewireEvent>) {
    self.changed(id, events, |node| node.update_params(obj));
  }

  pub fn update_props(&self, id: u32, props: &DictRef, events: &flume::Sender<PipewireEvent>) {
    self.changed(id, events, |node| node.update_props(props));
  }

  pub fn set_default(
    &self,
    kind: NodeType,
    name: Option<String>,
    events: &flume::Sender<PipewireEvent>,
  ) {
    let changed = match name {
      Some(name) => self.defaults.insert(kind, name.clone()) != Some(name),
      None => self.defaults.remove(&kind).is_some(),
    };

    if changed {
      let _ = events.send(PipewireEvent::AudioDefaultChanged(kind));
    }
  }

  pub fn set_target(&self, id: u32, name: Option<String>, events: &flume::Sender<PipewireEvent>) {
    let changed = match name {
      Some(name) => self.targets.insert(id, name.clone()) != Some(name),
      None => self.targets.remove(&id).is_some(),
    };

    if changed {
      let _ = events.send(PipewireEvent::AudioTargetChanged(id));
    }
  }

  fn changed(
    &self,
    id: u32,
    events: &flume::Sender<PipewireEvent>,
    update: impl FnOnce(&mut AudioNode) -> bool,
  ) {
    let Some(mut node) = self.nodes.get_mut(&id) else {
      return;
    };

    if update(&mut node) {
      drop(node);
      let _ = events.send(PipewireEvent::AudioNodeChanged(id));
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
