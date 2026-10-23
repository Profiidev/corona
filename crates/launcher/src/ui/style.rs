//! The handoff's launcher values on top of the active theme's tokens

use gpui_kit::{
  App, Background, Hsla, IntoElement, ParentElement, Styled, component::ActiveTheme, div, hsla,
  linear_color_stop, linear_gradient, px,
};

use crate::command::Tone;

pub const SEARCH_HEIGHT: f32 = 58.;
pub const FOOTER_HEIGHT: f32 = 42.;
pub const ROW_HEIGHT: f32 = 44.;
pub const HEADER_HEIGHT: f32 = 32.;
pub const EVALUATION_HEIGHT: f32 = 108.;
pub const TILE: f32 = 28.;
pub const RADIUS: f32 = 16.;
pub const ROW_RADIUS: f32 = 10.;
pub const PANEL_WIDTH: f32 = 320.;
pub const LIST_WIDTH: f32 = 330.;
pub const SIDEBAR_WIDTH: f32 = 230.;
pub const MAX_ARGS: usize = 3;

pub fn window_fill(cx: &App) -> Hsla {
  cx.theme().tokens.background.opacity(0.86)
}

pub fn popover_fill(cx: &App) -> Hsla {
  cx.theme().tokens.background.opacity(0.96)
}

pub fn footer_fill() -> Hsla {
  hsla(0., 0., 0., 0.14)
}

pub fn scrim() -> Hsla {
  hsla(0., 0., 0., 0.45)
}

fn foreground(alpha: f32, cx: &App) -> Hsla {
  cx.theme().foreground.opacity(alpha)
}

pub fn hover(cx: &App) -> Hsla {
  foreground(0.05, cx)
}

pub fn selected(cx: &App) -> Hsla {
  foreground(0.09, cx)
}

pub fn key_cap(cx: &App) -> Hsla {
  foreground(0.09, cx)
}

pub fn chip(cx: &App) -> (Hsla, Hsla) {
  let primary = cx.theme().colors.primary;
  (primary.opacity(0.38), primary.opacity(0.45))
}

pub fn focus_ring(cx: &App) -> Hsla {
  cx.theme().colors.ring
}

/// Fill and text of a tag
pub fn tone(tone: Tone, cx: &App) -> (Hsla, Hsla) {
  let theme = cx.theme();
  let tinted = |c: Hsla| (c.opacity(0.18), c);
  match tone {
    Tone::Ok => tinted(theme.colors.success),
    Tone::Warn => tinted(theme.colors.warning),
    Tone::Err => tinted(theme.colors.danger),
    Tone::Off => (foreground(0.08, cx), theme.colors.muted_foreground),
  }
}

/// The 145° tile gradient for a hue in degrees
pub fn tile(hue: f32) -> Background {
  let h = hue.rem_euclid(360.) / 360.;
  let h2 = (hue + 25.).rem_euclid(360.) / 360.;
  linear_gradient(
    145.,
    linear_color_stop(hsla(h, 0.62, 0.55, 1.), 0.),
    linear_color_stop(hsla(h2, 0.58, 0.36, 1.), 1.),
  )
}

/// A small key cap like `⌃ O`
pub fn kbd(keys: &str, cx: &App) -> impl IntoElement + use<> {
  let theme = cx.theme();
  let label = keys
    .split('-')
    .map(|k| match k {
      "ctrl" => "Ctrl".to_string(),
      "shift" => "Shift".to_string(),
      "alt" => "Alt".to_string(),
      "enter" => "↵".to_string(),
      "escape" => "Esc".to_string(),
      "tab" => "Tab".to_string(),
      "backspace" => "⌫".to_string(),
      k => k.to_uppercase(),
    })
    .collect::<Vec<_>>()
    .join(" ");
  div()
    .min_w(px(20.))
    .h(px(20.))
    .px(px(5.))
    .flex()
    .items_center()
    .justify_center()
    .rounded(px(5.))
    .bg(key_cap(cx))
    .text_size(px(11.))
    .text_color(theme.colors.muted_foreground)
    .child(label)
}
