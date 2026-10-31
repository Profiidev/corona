mod style;
mod view;

use anyhow::Result;
use corona_surface::overlay::open_on_focused;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Global, Pixels, Point,
  layer_shell::{KeyboardInteractivity, LayerShellOptions},
  px,
};

pub use view::Keyboard;

const NAMESPACE: &str = "corona_osk";

struct OskWindow(AnyWindowHandle);

impl Global for OskWindow {}

/// Where the keyboard was dragged to from its place at the bottom, for as long
/// as the shell runs
#[derive(Default)]
struct Offset(Point<Pixels>);

impl Global for Offset {}

pub fn is_open(cx: &App) -> bool {
  cx.has_global::<OskWindow>()
}

/// On the focused monitor; a screen-sized surface that only takes input on
/// the keyboard, so it can be dragged anywhere, and never keyboard focus, so
/// typing goes to the window that has it
pub fn open(cx: &mut App) -> Result<()> {
  if is_open(cx) {
    return Ok(());
  }
  let tweak = |layer: &mut LayerShellOptions| {
    // inside bars and docks, over nothing they reserve
    layer.exclusive_zone = Some(px(0.));
    layer.keyboard_interactivity = KeyboardInteractivity::None;
  };
  let handle = open_on_focused(NAMESPACE, tweak, cx, |_, cx| cx.new(Keyboard::new))?;
  cx.set_global(OskWindow(handle));
  Ok(())
}

pub fn close(cx: &mut App) {
  if !is_open(cx) {
    return;
  }
  let OskWindow(handle) = cx.remove_global::<OskWindow>();
  let _ = handle.update(cx, |_, window, _| window.remove_window());
}
