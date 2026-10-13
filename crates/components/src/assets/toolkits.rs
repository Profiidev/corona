//! Hands the active theme to GTK and Qt apps: the dark/light preference through
//! gsettings, the colors through `corona.css`, which `gtk.css` imports, and a
//! qt5ct/qt6ct color scheme.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use gpui_kit::{Hsla, component::ThemeColor};

/// Running apps follow the mode at once; colors apply when an app starts.
pub fn sync(dark: bool, colors: &ThemeColor) -> Result<()> {
  let scheme = if dark { "prefer-dark" } else { "prefer-light" };
  // on its own thread, so the shell neither waits for it nor leaves a zombie
  std::thread::spawn(move || {
    let status = std::process::Command::new("gsettings")
      .args(["set", "org.gnome.desktop.interface", "color-scheme", scheme])
      .status();
    if !matches!(status, Ok(s) if s.success()) {
      tracing::warn!("Failed to set the GTK color scheme: {status:?}");
    }
  });

  let config = dirs::config_dir().context("Failed to get config directory")?;
  let css = gtk_css(colors);
  for version in ["gtk-3.0", "gtk-4.0"] {
    let dir = config.join(version);
    write(&dir.join("corona.css"), &css)?;
    // a gtk.css that imports it already, like one home-manager writes, stays
    let gtk = dir.join("gtk.css");
    if !fs::read_to_string(&gtk).is_ok_and(|s| s.contains(IMPORT)) {
      write(&gtk, &format!("{IMPORT}\n"))?;
    }
  }
  let palette = qt_palette(colors);
  for qt in ["qt5ct", "qt6ct"] {
    write(&config.join(qt).join("colors/corona.conf"), &palette)?;
  }
  Ok(())
}

const IMPORT: &str = "@import url(\"corona.css\");";

fn write(path: &Path, content: &str) -> Result<()> {
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))
}

/// libadwaita's named colors, which adw-gtk3 reads too
fn gtk_css(c: &ThemeColor) -> String {
  let colors = [
    ("accent_color", c.primary),
    ("accent_bg_color", c.primary),
    ("accent_fg_color", c.primary_foreground),
    ("window_bg_color", c.background),
    ("window_fg_color", c.foreground),
    ("view_bg_color", c.background),
    ("view_fg_color", c.foreground),
    ("headerbar_bg_color", c.title_bar),
    ("headerbar_fg_color", c.foreground),
    ("sidebar_bg_color", c.sidebar),
    ("sidebar_fg_color", c.sidebar_foreground),
    ("card_bg_color", c.secondary),
    ("card_fg_color", c.secondary_foreground),
    ("popover_bg_color", c.popover),
    ("popover_fg_color", c.popover_foreground),
    ("dialog_bg_color", c.popover),
    ("dialog_fg_color", c.popover_foreground),
    ("destructive_color", c.danger),
    ("destructive_bg_color", c.danger),
    ("destructive_fg_color", c.danger_foreground),
    ("error_color", c.danger),
    ("warning_color", c.warning),
    ("success_color", c.success),
  ];
  let mut css = String::from("/* Written by corona, theme.sync_toolkits overwrites it */\n");
  for (name, color) in colors {
    let rgb = color.to_rgb();
    let [r, g, b] = [rgb.r, rgb.g, rgb.b].map(|v| (v * 255.).round() as u8);
    css += &format!("@define-color {name} rgba({r}, {g}, {b}, {:.3});\n", rgb.a);
  }
  css
}

/// A qt5ct/qt6ct color scheme: the QPalette roles in their enum order
fn qt_palette(c: &ThemeColor) -> String {
  let roles = |text: Hsla| {
    [
      text,                 // WindowText
      c.secondary,          // Button
      c.secondary,          // Light
      c.secondary,          // Midlight
      c.border,             // Dark
      c.border,             // Mid
      text,                 // Text
      c.primary_foreground, // BrightText
      text,                 // ButtonText
      c.background,         // Base
      c.background,         // Window
      c.border,             // Shadow
      c.primary,            // Highlight
      c.primary_foreground, // HighlightedText
      c.link,               // Link
      c.link,               // LinkVisited
      c.muted,              // AlternateBase
      c.background,         // NoRole
      c.popover,            // ToolTipBase
      c.popover_foreground, // ToolTipText
      c.muted_foreground,   // PlaceholderText
    ]
    .map(argb)
    .join(", ")
  };
  let active = roles(c.foreground);
  format!(
    "[ColorScheme]\nactive_colors={active}\ninactive_colors={active}\ndisabled_colors={}\n",
    roles(c.muted_foreground)
  )
}

/// Qt's `#AARRGGBB`
fn argb(color: Hsla) -> String {
  let rgb = color.to_rgb();
  let [a, r, g, b] = [rgb.a, rgb.r, rgb.g, rgb.b].map(|v| (v * 255.).round() as u8);
  format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
  use gpui_kit::hsla;

  use super::*;

  fn colors() -> ThemeColor {
    ThemeColor {
      primary: hsla(0., 1., 0.5, 1.),
      background: hsla(0., 0., 1., 0.5),
      ..Default::default()
    }
  }

  #[test]
  fn gtk_css_defines_rgba_colors() {
    let css = gtk_css(&colors());
    assert!(
      css.contains("@define-color accent_bg_color rgba(255, 0, 0, 1.000);"),
      "{css}"
    );
    assert!(
      css.contains("@define-color window_bg_color rgba(255, 255, 255, 0.500);"),
      "{css}"
    );
  }

  #[test]
  fn qt_palette_has_every_role_alpha_first() {
    let palette = qt_palette(&colors());
    let lines: Vec<_> = palette.lines().collect();
    assert_eq!(lines[0], "[ColorScheme]");
    for (line, key) in lines[1..].iter().zip(["active", "inactive", "disabled"]) {
      let roles: Vec<_> = line
        .strip_prefix(&format!("{key}_colors="))
        .unwrap()
        .split(", ")
        .collect();
      assert_eq!(roles.len(), 21);
      assert_eq!(roles[12], "#ffff0000", "Highlight");
      assert_eq!(roles[10], "#80ffffff", "Window");
    }
  }
}
