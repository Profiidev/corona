use std::collections::HashMap;

use crate::integration::pipewire::state::{AudioNode, NodeType};

#[derive(Debug, Clone)]
pub enum AudioEvent {
  Nodes(NodeType, Vec<AudioNode>),
  DefaultSink(Option<AudioNode>),
  DefaultSource(Option<AudioNode>),
  Targets(HashMap<u32, u32>),
}
