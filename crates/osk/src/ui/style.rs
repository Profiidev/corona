//! The handoff's keyboard values on top of the active theme's tokens

use gpui_kit::{App, BoxShadow, Hsla, component::ActiveTheme, hsla, px};

pub const WIDTH: f32 = 1100.;
pub const KEY_HEIGHT: f32 = 52.;
pub const GAP: f32 = 6.;
pub const KEY_RADIUS: f32 = 9.;
pub const STRIP_HEIGHT: f32 = 40.;
/// The drag handle above the strip
pub const GRIP_HEIGHT: f32 = 14.;
pub const PADDING_X: f32 = 12.;
pub const PADDING_BOTTOM: f32 = 12.;
pub const RADIUS: f32 = 20.;
/// Between the keyboard and the screen's edges
pub const SCREEN_GAP: f32 = 16.;
pub const ROWS: usize = 5;
pub const HEIGHT: f32 =
  GRIP_HEIGHT + STRIP_HEIGHT + ROWS as f32 * (KEY_HEIGHT + GAP) + PADDING_BOTTOM;
pub const POPUP_KEY_WIDTH: f32 = 42.;
pub const POPUP_KEY_HEIGHT: f32 = 46.;
pub const POPUP_PADDING: f32 = 6.;
/// From the key's bottom edge to the accent popup's
pub const POPUP_BOTTOM: f32 = 58.;
/// How far the accent popup reaches above the keyboard at most
pub const POPUP_REACH: f32 = POPUP_BOTTOM + POPUP_KEY_HEIGHT + 2. * POPUP_PADDING;

pub fn card_fill(cx: &App) -> Hsla {
  cx.theme().tokens.background.opacity(1.)
}

pub fn popup_fill(cx: &App) -> Hsla {
  cx.theme().tokens.background.opacity(1.)
}

fn foreground(alpha: f32, cx: &App) -> Hsla {
  cx.theme().foreground.opacity(alpha)
}

pub fn char_key(cx: &App) -> Hsla {
  foreground(0.09, cx)
}

pub fn special_key(cx: &App) -> Hsla {
  foreground(0.05, cx)
}

pub fn pressed_key(cx: &App) -> Hsla {
  foreground(0.22, cx)
}

pub fn key_border(cx: &App) -> Hsla {
  foreground(0.08, cx)
}

/// A latched modifier, Caps on, or a key showing its AltGr character
pub fn active_key(cx: &App) -> Hsla {
  cx.theme().colors.primary.opacity(0.7)
}

pub fn enter_key(cx: &App) -> Hsla {
  cx.theme().colors.primary.opacity(0.55)
}

pub fn best_chip(cx: &App) -> Hsla {
  foreground(0.1, cx)
}

pub fn hover(cx: &App) -> Hsla {
  foreground(0.05, cx)
}

pub fn grip(cx: &App) -> Hsla {
  foreground(0.25, cx)
}

/// The edge under a key, `pressed` sinks it
pub fn key_shadow(pressed: bool) -> Vec<BoxShadow> {
  let depth = if pressed { 1. } else { 2. };
  vec![BoxShadow::new(px(0.), px(depth), hsla(0., 0., 0., 0.35))]
}
