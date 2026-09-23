use anyhow::Result;
use gpui_kit::App;

pub mod compositor;
pub mod dbus;
pub mod desktop;
pub mod pipewire;

pub fn init(cx: &mut App) -> Result<()> {
  compositor::init(cx)?;
  cx.foreground_executor().clone().block_on(dbus::init(cx))?;
  pipewire::init(cx)
}
