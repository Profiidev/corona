mod style;
mod view;

use anyhow::Result;
use corona_surface::overlay::open_on_focused;
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Entity, Global, Pixels, Point,
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

/// The compositor can destroy the surface, which must not leave the keyboard
/// marked as shown
pub fn watch_closed(visible: Entity<bool>, cx: &mut App) {
  cx.on_window_closed(move |cx, id| {
    if cx
      .try_global::<OskWindow>()
      .is_some_and(|w| w.0.window_id() == id)
    {
      cx.remove_global::<OskWindow>();
      visible.write_changed(cx, false);
    }
  })
  .detach();
}

pub fn close(cx: &mut App) {
  if !is_open(cx) {
    return;
  }
  let OskWindow(handle) = cx.remove_global::<OskWindow>();
  let _ = handle.update(cx, |_, window, _| window.remove_window());
}
