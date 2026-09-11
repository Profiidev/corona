use anyhow::Result;
use gpui_kit::App;

pub mod compositor;
pub mod desktop;
pub mod pipewire;

pub fn init(cx: &mut App) -> Result<()> {
  compositor::init(cx)?;
  pipewire::init(cx)
}
