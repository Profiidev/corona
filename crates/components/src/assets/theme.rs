use anyhow::{Context, Result};
use gpui_kit::{
  App, SharedString,
  component::{ActiveTheme, Theme, ThemeMode, ThemeRegistry},
};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn load(cx: &mut App) -> Result<()> {
  let theme = SharedString::new(&cx.global::<corona_config::Config>().theme);
  let registry = ThemeRegistry::global_mut(cx);

  for file in THEMES.files() {
    let content = file.contents_utf8().context("Failed to read theme file")?;
    registry.load_themes_from_str(content)?;
  }

  let themes = registry.themes();
  let config = themes.get(&theme).context("Failed to get theme")?.clone();
  let other = counterpart(&theme).and_then(|name| themes.get(name.as_str()).cloned());

  let theme = Theme::global_mut(cx);
  theme.apply_config(&config);
  if let Some(other) = other {
    match other.mode.is_dark() {
      true => theme.dark_theme = other,
      false => theme.light_theme = other,
    }
  }

  Ok(())
}

fn counterpart(name: &str) -> Option<String> {
  if let Some(base) = name.strip_suffix(" Dark") {
    return Some(format!("{base} Light"));
  }
  name
    .strip_suffix(" Light")
    .map(|base| format!("{base} Dark"))
}

pub fn toggle_mode(cx: &mut App) {
  let mode = match cx.theme().is_dark() {
    true => ThemeMode::Light,
    false => ThemeMode::Dark,
  };
  Theme::change(mode, None, cx);
}
