use crate::{
  adapter::{gpu::GpuError, slint::SlintCustomPlatformError, wayland::WaylandAdapterError},
  event::event_loop::EventLoopError,
};

#[derive(Debug, thiserror::Error)]
pub enum CoronaError {
  #[error("Wayland adapter error: {0}")]
  WaylandAdapterError(#[from] WaylandAdapterError),
  #[error("GPU context error: {0}")]
  GpuError(#[from] GpuError),
  #[error("Slint platform error: {0}")]
  SlintPlatformError(#[from] SlintCustomPlatformError),
  #[error("Slint platform error: {0}")]
  EventLoopError(#[from] EventLoopError),
  #[error("Event loop already taken")]
  EventLoopTaken,
}
