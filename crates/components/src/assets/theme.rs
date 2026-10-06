use anyhow::{Context, Result};
use gpui_kit::{
  App,
  component::{ActiveTheme, Theme, ThemeMode, ThemeRegistry},
};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn load(cx: &mut App) -> Result<()> {
  let registry = ThemeRegistry::global_mut(cx);
  for file in THEMES.files() {
    let content = file.contents_utf8().context("Failed to read theme file")?;
    registry.load_themes_from_str(content)?;
  }

  let theme = cx.global::<corona_config::Config>().theme.clone();
  apply(&theme, cx)
}

/// Switches to the named theme, and to its Light/Dark counterpart for the other mode.
pub fn apply(name: &str, cx: &mut App) -> Result<()> {
  let themes = ThemeRegistry::global(cx).themes();
  let config = themes
    .get(name)
    .with_context(|| format!("unknown theme: {name}"))?
    .clone();
  let other = counterpart(name).and_then(|name| themes.get(name.as_str()).cloned());

  Theme::update(cx, |theme| {
    theme.apply_config(&config);
    if let Some(other) = other {
      match other.mode.is_dark() {
        true => theme.dark_theme = other,
        false => theme.light_theme = other,
      }
    }
  });
  Ok(())
}

pub fn names(cx: &App) -> Vec<String> {
  let mut names: Vec<_> = ThemeRegistry::global(cx)
    .themes()
    .keys()
    .map(|name| name.to_string())
    .collect();
  names.sort();
  names
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
