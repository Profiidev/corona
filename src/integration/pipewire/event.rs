use gpui_kit::EventEmitter;

use crate::integration::pipewire::state::NodeType;

pub struct PipewireEventEmitter;

impl EventEmitter<PipewireEvent> for PipewireEventEmitter {}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
pub enum PipewireEvent {
  AudioNodeAdded(u32),
  AudioNodeChanged(u32),
  AudioNodeRemoved(u32),
  AudioDefaultChanged(NodeType),
  AudioTargetChanged(u32),
}
