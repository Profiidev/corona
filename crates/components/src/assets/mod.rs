use anyhow::Result;
use gpui_kit::App;

pub mod icons;
mod theme;

pub use icons::Assets;
pub use theme::{apply as apply_theme, names as theme_names, toggle_mode};

pub fn load(cx: &mut App) -> Result<()> {
  theme::load(cx)
}
