use gpui_kit::EventEmitter;

pub struct PipewireEventEmitter;

impl EventEmitter<PipewireEvent> for PipewireEventEmitter {}

#[derive(Debug, Clone, Copy)]
pub enum PipewireEvent {
  AudioNodeAdded(u32),
  AudioNodeChanged(u32),
  AudioNodeRemoved(u32),
}
