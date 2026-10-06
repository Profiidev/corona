use anyhow::{Context, Result};
use corona_config::{ConfigProvider, ThemeConfig, observe_section};
use gpui_kit::{
  App, SharedString,
  component::{ActiveTheme, Theme, ThemeMode, ThemeRegistry},
  px,
};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn load(cx: &mut App) -> Result<()> {
  let registry = ThemeRegistry::global_mut(cx);
  for file in THEMES.files() {
    let content = file.contents_utf8().context("Failed to read theme file")?;
    registry.load_themes_from_str(content)?;
  }

  // a bad theme name keeps the built-in theme instead of stopping the shell
  let apply = |theme: &ThemeConfig, cx: &mut App| {
    if let Err(e) = apply_config(theme, cx) {
      tracing::error!("Failed to apply theme: {e:#}");
    }
  };
  let theme = cx.config().theme.clone();
  apply(&theme, cx);
  observe_section(cx, |c| &c.theme, apply);
  Ok(())
}

/// The `[theme]` settings: the named theme, then the mode if one is set.
fn apply_config(theme: &ThemeConfig, cx: &mut App) -> Result<()> {
  apply(&theme.name, cx)?;
  if let Some(mode) = theme.mode {
    let mode = match mode {
      corona_config::ThemeMode::Dark => ThemeMode::Dark,
      corona_config::ThemeMode::Light => ThemeMode::Light,
    };
    Theme::change(mode, None, cx);
  }
  apply_style(theme, cx);
  Ok(())
}

/// The look settings over the active theme file. Every value is set, from the
/// file or gpui's default, since a file that leaves one out would otherwise keep
/// the last theme's, scaled again.
fn apply_style(style: &ThemeConfig, cx: &mut App) {
  let theme = Theme::global(cx);
  let file = match theme.mode.is_dark() {
    true => theme.dark_theme.clone(),
    false => theme.light_theme.clone(),
  };
  let scale = style.corner_radius_scale.max(0.);
  let radius = file.radius.map_or(6., |r| r as f32) * scale;
  let radius_lg = file.radius_lg.map_or(8., |r| r as f32) * scale;
  let font_size = file.font_size.unwrap_or(16.) * style.font_scale.max(0.1);
  let font_family: SharedString = match &style.font_family {
    Some(family) => family.clone().into(),
    None => file.font_family.clone().unwrap_or(".SystemUIFont".into()),
  };
  Theme::update(cx, |theme| {
    theme.radius = px(radius);
    theme.radius_lg = px(radius_lg);
    theme.font_size = px(font_size);
    theme.font_family = font_family;
    theme.shadow = style.shadow && file.shadow.unwrap_or(true);
  });
}

/// Switches to the named theme, and to its Light/Dark counterpart for the other mode.
fn apply(name: &str, cx: &mut App) -> Result<()> {
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
  let dark = cx.theme().is_dark();
  if let Err(e) = set_mode(!dark, cx) {
    tracing::error!("Failed to switch the theme mode: {e:#}");
  }
}

/// Switches to dark or light mode and keeps it in the settings.
pub fn set_mode(dark: bool, cx: &mut App) -> Result<()> {
  let mode = match dark {
    true => corona_config::ThemeMode::Dark,
    false => corona_config::ThemeMode::Light,
  };
  corona_config::update(cx, |c| c.theme.mode = Some(mode))
}

/// Switches to the named theme, in its own mode, and keeps it in the settings.
pub fn set_theme(name: String, cx: &mut App) -> Result<()> {
  if !ThemeRegistry::global(cx)
    .themes()
    .contains_key(name.as_str())
  {
    anyhow::bail!("unknown theme: {name}");
  }
  corona_config::update(cx, |c| {
    c.theme.name = name;
    c.theme.mode = None;
  })
}
