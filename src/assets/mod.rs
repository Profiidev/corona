use anyhow::Result;
use gpui_kit::App;

pub mod icons;
mod theme;

pub use icons::Assets;

pub fn load(cx: &mut App) -> Result<()> {
  theme::load(cx)
}
