use gpui_kit::EventEmitter;

pub struct PipewireEventEmitter;

impl EventEmitter<PipewireEvent> for PipewireEventEmitter {}

#[derive(Debug, Clone, Copy)]
pub enum PipewireEvent {
  AudioSinkAdded(u32),
  AudioSinkChanged(u32),
  AudioSinkRemoved(u32),
}
