use anyhow::{Context, Result};
use gpui_kit::{
  App, SharedString,
  component::{Theme, ThemeRegistry},
};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn load(cx: &mut App) -> Result<()> {
  let theme = SharedString::new(&cx.global::<crate::config::Config>().theme);
  let registry = ThemeRegistry::global_mut(cx);

  for file in THEMES.files() {
    let content = file.contents_utf8().context("Failed to read theme file")?;
    registry.load_themes_from_str(content)?;
  }

  let theme = registry
    .themes()
    .get(&theme)
    .context("Failed to get theme")?
    .clone();

  Theme::global_mut(cx).apply_config(&theme);

  Ok(())
}
