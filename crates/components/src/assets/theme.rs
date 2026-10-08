use anyhow::{Context, Result};
use corona_config::{ConfigProvider, ThemeConfig, observe_section};
use gpui_kit::{
  App, SharedString,
  component::{ActiveTheme, Theme, ThemeMode, ThemeRegistry},
  px,
};
use include_dir::{Dir, include_dir};

const THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../assets/themes");

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
  let theme_font: SharedString = file.font_family.clone().unwrap_or(".SystemUIFont".into());
  Theme::update(cx, |theme| {
    theme.radius = px(radius);
    theme.radius_lg = px(radius_lg);
    theme.font_size = px(font_size);
    theme.font_family = theme_font;
    theme.shadow = style.shadow && file.shadow.unwrap_or(true);
  });
  // gpui has resolved the theme's font to a real family by now, like
  // ".SystemUIFont" to "DejaVu Sans"; that is what applies without an override
  cx.set_global(ThemeFont(cx.theme().font_family.clone()));
  if let Some(family) = &style.font_family {
    let family: SharedString = family.clone().into();
    Theme::update(cx, |theme| theme.font_family = family);
  }
}

struct ThemeFont(SharedString);

impl gpui_kit::Global for ThemeFont {}

/// The font of the active theme, the one used when no font is set
pub fn theme_font(cx: &App) -> SharedString {
  cx.try_global::<ThemeFont>()
    .map(|f| f.0.clone())
    .unwrap_or_else(|| cx.theme().font_family.clone())
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

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  const DEFAULT: &str = "shadcn Zinc Blue Dark";

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(Config::default());
      load(cx).unwrap();
    });
  }

  /// Keeps `corona_config::update` inside a temp dir
  fn temp_dirs() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
      std::env::set_var("XDG_STATE_HOME", dir.path().join("state"));
    }
    dir
  }

  #[test]
  fn counterpart_swaps_the_mode_suffix() {
    for (name, other) in [
      ("Ayu Dark", Some("Ayu Light")),
      ("Ayu Light", Some("Ayu Dark")),
      ("A B Dark", Some("A B Light")),
      ("Dark", None),
      ("Light", None),
      ("Ayu", None),
      ("Ayu dark", None),
      ("", None),
    ] {
      assert_eq!(counterpart(name).as_deref(), other, "{name}");
    }
  }

  #[gpui::test]
  fn every_embedded_theme_loads(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let names = names(cx);
      assert!(names.len() >= THEMES.files().count());
      assert!(names.is_sorted());
      assert!(names.iter().any(|n| n == DEFAULT));
      // the default config applies without error
      assert_eq!(cx.theme().theme_name(), DEFAULT);
    });
  }

  #[gpui::test]
  fn apply_unknown_theme_fails(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let err = apply("No Such Theme", cx).unwrap_err();
      assert!(err.to_string().contains("No Such Theme"));
      assert_eq!(cx.theme().theme_name(), DEFAULT);
    });
  }

  #[gpui::test]
  fn apply_pairs_the_counterpart(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let names = names(cx);
      let pair = names
        .iter()
        .find_map(|n| {
          let other = counterpart(n)?;
          (n.ends_with(" Dark") && names.contains(&other)).then(|| (n.clone(), other))
        })
        .expect("a Dark/Light theme pair");
      for name in [&pair.0, &pair.1] {
        apply(name, cx).unwrap();
        assert_eq!(cx.theme().dark_theme.name.as_ref(), pair.0);
        assert_eq!(cx.theme().light_theme.name.as_ref(), pair.1);
      }
    });
  }

  #[gpui::test]
  fn style_values_are_clamped(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let base_size = cx.theme().dark_theme.font_size.unwrap_or(16.);
      let style = ThemeConfig {
        corner_radius_scale: -2.,
        font_scale: 0.,
        shadow: false,
        ..Default::default()
      };
      apply_style(&style, cx);
      assert_eq!(cx.theme().radius, px(0.));
      assert_eq!(cx.theme().radius_lg, px(0.));
      assert!((cx.theme().font_size.as_f32() - base_size * 0.1).abs() < 1e-3);
      assert!(!cx.theme().shadow);

      apply_style(&ThemeConfig::default(), cx);
      assert!(cx.theme().radius > px(0.));
      assert!((cx.theme().font_size.as_f32() - base_size).abs() < 1e-3);
    });
  }

  #[gpui::test]
  fn font_override_keeps_the_theme_font(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let own = theme_font(cx);
      let style = ThemeConfig {
        font_family: Some("Some Font".into()),
        ..Default::default()
      };
      apply_style(&style, cx);
      assert_eq!(cx.theme().font_family.as_ref(), "Some Font");
      assert_eq!(theme_font(cx), own);
      apply_style(&ThemeConfig::default(), cx);
      assert_eq!(cx.theme().font_family, own);
    });
  }

  #[gpui::test]
  fn theme_font_falls_back_to_the_active_font(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      assert_eq!(theme_font(cx), cx.theme().font_family);
    });
  }

  #[gpui::test]
  fn config_changes_reapply(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.theme.corner_radius_scale = 0.;
      config.theme.mode = Some(corona_config::ThemeMode::Light);
      cx.set_global(config);
    });
    cx.run_until_parked();
    cx.update(|cx| {
      assert_eq!(cx.theme().radius, px(0.));
      assert!(!cx.theme().is_dark());
      // a bad name keeps the theme it had
      let mut config = cx.config().clone();
      config.theme.name = "No Such Theme".into();
      cx.set_global(config);
    });
    cx.run_until_parked();
    cx.update(|cx| assert!(!cx.theme().is_dark()));
  }

  #[gpui::test]
  fn set_theme_rejects_unknown_and_clears_mode(cx: &mut TestAppContext) {
    let _dir = temp_dirs();
    setup(cx);
    cx.update(|cx| {
      assert!(set_theme("No Such Theme".into(), cx).is_err());
      set_mode(false, cx).unwrap();
      assert_eq!(
        cx.config().theme.mode,
        Some(corona_config::ThemeMode::Light)
      );
      let other = names(cx).into_iter().find(|n| n != DEFAULT).unwrap();
      set_theme(other.clone(), cx).unwrap();
      assert_eq!(cx.config().theme.name, other);
      assert_eq!(cx.config().theme.mode, None);
    });
  }

  #[gpui::test]
  fn toggle_mode_flips_and_persists(cx: &mut TestAppContext) {
    let dir = temp_dirs();
    setup(cx);
    cx.update(|cx| assert!(cx.theme().is_dark()));
    cx.update(toggle_mode);
    cx.run_until_parked();
    cx.update(|cx| {
      assert_eq!(
        cx.config().theme.mode,
        Some(corona_config::ThemeMode::Light)
      );
      assert!(!cx.theme().is_dark());
    });
    cx.update(toggle_mode);
    cx.run_until_parked();
    cx.update(|cx| assert!(cx.theme().is_dark()));
    let settings = std::fs::read_to_string(dir.path().join("state/corona/settings.toml")).unwrap();
    assert!(settings.contains("dark"), "{settings}");
  }
}
