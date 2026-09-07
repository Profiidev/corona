use gpui_kit::EventEmitter;

use crate::compositor::types;

pub struct CompositorEventEmitter;

impl EventEmitter<CompositorEvent> for CompositorEventEmitter {}

pub enum CompositorEvent {
  WorkspaceChanged(Vec<types::Workspace>),
}
