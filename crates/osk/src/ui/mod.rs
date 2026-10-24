mod style;
mod view;

use anyhow::{Context as _, Result};
use corona_compositor::CompositorExt;
use corona_config::APP_NAME;
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Global, Pixels, Point, Styled,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px, size,
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
  let monitor = cx.compositor().active_monitor(cx).name.clone();
  let display = display_id_for(&monitor, cx).context("no display for the focused monitor")?;
  let options = WindowOptions {
    kind: WindowKind::LayerShell(LayerShellOptions {
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
      // inside bars and docks, over nothing they reserve
      exclusive_zone: Some(px(0.)),
      exclusive_edge: None,
      margin: None,
      layer: Layer::Overlay,
      namespace: NAMESPACE.to_string(),
      keyboard_interactivity: KeyboardInteractivity::None,
    }),
    window_background: WindowBackgroundAppearance::Transparent,
    window_decorations: Some(WindowDecorations::Client),
    app_id: Some(APP_NAME.to_string()),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: Point::default(),
      size: size(px(0.), px(0.)),
    })),
    display_id: Some(display),
    ..Default::default()
  };
  let handle = cx.open_window(options, |window, cx| {
    let keyboard = cx.new(Keyboard::new);
    cx.new(|cx| Root::new(keyboard, window, cx).bg(gpui_kit::transparent_black()))
  })?;
  cx.set_global(OskWindow(handle.into()));
  Ok(())
}

pub fn close(cx: &mut App) {
  if !is_open(cx) {
    return;
  }
  let OskWindow(handle) = cx.remove_global::<OskWindow>();
  let _ = handle.update(cx, |_, window, _| window.remove_window());
}
