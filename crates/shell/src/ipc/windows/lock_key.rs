use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;

pub use crate::osd::keys::lock_keys::LockKey;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<Pressed>();
}

/// A lock key was pressed; bound to the key in the compositor, since only it
/// sees the key
pub struct Pressed;

impl IpcCommand for Pressed {
  const COMMAND: &'static str = "lock-key:pressed";

  type Payload = LockKey;
  type Response = ();

  fn handle(key: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    crate::osd::keys::lock_keys::pressed(key, cx);
    Ok(())
  }
}
