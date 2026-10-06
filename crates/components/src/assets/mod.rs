use anyhow::Result;
use gpui_kit::App;

pub mod icons;
mod theme;

pub use icons::Assets;
pub use theme::{names as theme_names, set_mode, set_theme, theme_font, toggle_mode};

pub fn load(cx: &mut App) -> Result<()> {
  theme::load(cx)
}
