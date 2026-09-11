use std::rc::Rc;

use dashmap::DashMap;
use pipewire::{
  node::{Node, NodeListener},
  spa::utils::dict::DictRef,
};

#[derive(Clone)]
pub struct PipewireState {
  pub audio: AudioState,
  pub video: VideoState,
}

impl PipewireState {
  pub fn new() -> Self {
    Self {
      audio: AudioState {
        sinks: Rc::new(DashMap::new()),
      },
      video: VideoState {},
    }
  }
}

#[derive(Clone)]
pub struct AudioState {
  pub sinks: Rc<DashMap<u32, AudioSink>>,
}

pub struct AudioSink {
  pub id: u32,
  pub name: String,
  pub description: String,
  pub nickname: Option<String>,
  pub device: u32,
  pub volumes: Vec<f32>,
  pub mute: bool,
  pub node: Node,
  pub listener: NodeListener,
}

impl AudioSink {
  pub fn new(id: u32, props: &DictRef, node: Node, listener: NodeListener) -> Option<Self> {
    Some(Self {
      id,
      name: props.get("node.name")?.to_string(),
      description: props.get("node.description")?.to_string(),
      nickname: props.get("node.nick").map(|s| s.to_string()),
      device: props.get("device.id").and_then(|s| s.parse().ok())?,
      mute: false,
      volumes: vec![0.0],
      node,
      listener,
    })
  }
}

#[derive(Clone)]
pub struct VideoState {}
