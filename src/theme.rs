use anyhow::{Context, Result};
use gpui_kit::{App, component::ThemeRegistry};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn load_themes(cx: &mut App) -> Result<()> {
  let registry = ThemeRegistry::global_mut(cx);

  for file in THEMES.files() {
    let content = file.contents_utf8().context("Failed to read theme file")?;
    registry.load_themes_from_str(content)?;
  }

  Ok(())
}
