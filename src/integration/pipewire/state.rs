use std::{str::FromStr, sync::Arc};

use anyhow::{Result, anyhow, bail};
use dashmap::DashMap;
use pipewire::spa::{
  pod::{Object, Value, ValueArray},
  sys,
  utils::dict::DictRef,
};
use tracing::warn;

use crate::integration::pipewire::event::PipewireEvent;

#[derive(Clone)]
pub struct PipewireState {
  pub audio: AudioState,
}

impl PipewireState {
  pub fn new() -> Self {
    Self {
      audio: AudioState {
        sinks: Arc::new(DashMap::new()),
      },
    }
  }
}

#[derive(Clone)]
pub struct AudioState {
  pub sinks: Arc<DashMap<u32, AudioSink>>,
}

#[derive(Clone, Debug)]
pub struct AudioSink {
  pub id: u32,
  pub name: String,
  pub description: String,
  pub nickname: Option<String>,
  pub device: u32,
  pub profile_device: Option<i32>,
  pub volumes: Vec<f32>,
  pub mute: bool,
}

impl AudioSink {
  pub fn new(id: u32, props: &DictRef) -> Option<Self> {
    Some(Self {
      id,
      name: props.get("node.name")?.to_string(),
      description: props.get("node.description")?.to_string(),
      nickname: props.get("node.nick").map(|s| s.to_string()),
      device: props.get("device.id").and_then(|s| s.parse().ok())?,
      profile_device: None,
      mute: false,
      volumes: vec![0.0],
    })
  }

  fn update_props(&mut self, props: &DictRef) -> bool {
    let mut changed = false;
    changed |= update(&mut self.name, props.get("node.name"));
    changed |= update(&mut self.description, props.get("node.description"));
    changed |= update_optional(&mut self.nickname, props.get("node.nick"));
    changed |= update(&mut self.device, props.get("device.id"));
    changed |= update_optional(&mut self.profile_device, props.get("card.profile.device"));
    changed
  }

  fn update(&mut self, obj: Object) -> bool {
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

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum NodeType {
  AudioSink,
}

impl NodeType {
  pub fn update(
    &self,
    state: &PipewireState,
    id: u32,
    obj: Object,
    events: &flume::Sender<PipewireEvent>,
  ) {
    match self {
      NodeType::AudioSink => {
        let Some(mut sink) = state.audio.sinks.get_mut(&id) else {
          return;
        };

        if sink.update(obj) {
          drop(sink);
          let _ = events.send(PipewireEvent::AudioSinkChanged(id));
        }
      }
    }
  }

  pub fn update_props(
    &self,
    state: &PipewireState,
    id: u32,
    props: &DictRef,
    events: &flume::Sender<PipewireEvent>,
  ) {
    match self {
      NodeType::AudioSink => {
        let Some(mut sink) = state.audio.sinks.get_mut(&id) else {
          return;
        };

        if sink.update_props(props) {
          drop(sink);
          let _ = events.send(PipewireEvent::AudioSinkChanged(id));
        }
      }
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

impl FromStr for NodeType {
  type Err = anyhow::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "Audio/Sink" => Ok(NodeType::AudioSink),
      _ => Err(anyhow!("Invalid node type: {}", s)),
    }
  }
}

#[cfg(test)]
mod tests {
  use pipewire::spa::{static_dict, utils::dict::StaticDict};

  use super::*;

  fn sink() -> AudioSink {
    static PROPS: StaticDict = static_dict! {
      "node.name" => "alsa_output.speaker",
      "node.description" => "Speaker",
      "node.nick" => "Built-in",
      "device.id" => "47",
    };

    AudioSink::new(54, &PROPS).expect("the registry props are complete")
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
    assert_eq!(sink.device, 47);
  }
}
