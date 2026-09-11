use std::{str::FromStr, sync::Arc};

use anyhow::anyhow;
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
      mute: false,
      volumes: vec![0.0],
    })
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
