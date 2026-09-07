use gpui_kit::EventEmitter;

use crate::compositor::types;

pub struct CompositorEventEmitter;

impl EventEmitter<CompositorEvent> for CompositorEventEmitter {}

pub enum CompositorEvent {
  Workspace(Vec<types::Workspace>),
  ActiveWorkspace(types::Workspace),
  ActiveScratchpad(types::Monitor),
  Monitor(Vec<types::Monitor>),
  ActiveMonitor(types::Monitor),
  Window(Vec<types::Window>),
  ActiveWindow(types::Window),
}
