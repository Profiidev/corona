mod form;
pub mod hud;
mod launcher;
mod overlays;
mod rows;
mod style;

use anyhow::{Context as _, Result};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider};
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Global, KeyBinding, Pixels, Point, Size, Styled,
  WeakEntity, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px, size,
};

pub use launcher::Launcher;

use crate::view::View;

const NAMESPACE: &str = "corona_launcher";
const CONTEXT: &str = "Launcher";
pub const WIDTH: f32 = 720.;
pub const HEIGHT: f32 = 480.;
/// With the preview pane
pub const WIDE: f32 = 900.;
pub const TALL: f32 = 500.;

gpui_kit::actions!(
  launcher,
  [
    SelectNext,
    SelectPrev,
    SelectLeft,
    SelectRight,
    PageDown,
    PageUp,
    Run,
    RunSecondary,
    RunKeepOpen,
    Complete,
    CompleteBack,
    Back,
    Pop,
    ToggleActions,
    TogglePreview,
    Submit,
  ]
);

/// Bound in `Launcher > Input` too, so they outrank the text field's own keys
pub fn bind_keys(cx: &mut App) {
  for context in [CONTEXT, "Launcher > Input"] {
    let c = Some(context);
    cx.bind_keys([
      KeyBinding::new("down", SelectNext, c),
      KeyBinding::new("ctrl-j", SelectNext, c),
      KeyBinding::new("ctrl-n", SelectNext, c),
      KeyBinding::new("up", SelectPrev, c),
      KeyBinding::new("ctrl-k", SelectPrev, c),
      KeyBinding::new("left", SelectLeft, c),
      KeyBinding::new("ctrl-h", SelectLeft, c),
      KeyBinding::new("right", SelectRight, c),
      KeyBinding::new("ctrl-l", SelectRight, c),
      KeyBinding::new("ctrl-d", PageDown, c),
      KeyBinding::new("pagedown", PageDown, c),
      KeyBinding::new("ctrl-u", PageUp, c),
      KeyBinding::new("pageup", PageUp, c),
      KeyBinding::new("enter", Run, c),
      KeyBinding::new("ctrl-enter", RunSecondary, c),
      KeyBinding::new("ctrl-shift-enter", RunKeepOpen, c),
      KeyBinding::new("tab", Complete, c),
      KeyBinding::new("shift-tab", CompleteBack, c),
      KeyBinding::new("escape", Back, c),
      KeyBinding::new("ctrl-o", ToggleActions, c),
      KeyBinding::new("ctrl-p", TogglePreview, c),
      KeyBinding::new("ctrl-s", Submit, c),
    ]);
  }
}

/// How the launcher opens
pub enum Open {
  /// The root search, optionally with a query
  Root { query: String },
  /// Straight into a command, as a per-command keybind does
  Command { id: String, query: String },
  /// Straight into a view, without root search behind it (dmenu)
  View(View),
}

pub struct LauncherState {
  handle: AnyWindowHandle,
  view: WeakEntity<Launcher>,
}

impl Global for LauncherState {}

pub fn is_open(cx: &App) -> bool {
  cx.has_global::<LauncherState>()
}

pub fn toggle(cx: &mut App) -> Result<()> {
  if is_open(cx) {
    close(cx);
    return Ok(());
  }
  open(
    Open::Root {
      query: String::new(),
    },
    cx,
  )
}

pub fn close(cx: &mut App) {
  if !cx.has_global::<LauncherState>() {
    return;
  }
  let state = cx.remove_global::<LauncherState>();
  let _ = state
    .handle
    .update(cx, |_, window, _| window.remove_window());
}

pub fn open(open: Open, cx: &mut App) -> Result<()> {
  if let Some((handle, view)) = cx
    .try_global::<LauncherState>()
    .and_then(|s| Some((s.handle, s.view.upgrade()?)))
  {
    return handle.update(cx, |_, window, cx| {
      view.update(cx, |view, cx| view.reset(open, window, cx));
      view.update(cx, |view, cx| view.focus(window, cx));
    });
  }

  let monitor = cx.compositor().active_monitor(cx).clone();
  let display = display_id_for(&monitor.name, cx).context("no display for the focused monitor")?;
  let scale = monitor.scale.max(0.1);
  let height = monitor.height as f32 / scale;
  // the window's centre on the upper third line
  let top = (height / 3. - HEIGHT / 2.).max(24.);
  let unfocus = cx.config().launcher.close_on_unfocus;

  let options = WindowOptions {
    kind: WindowKind::LayerShell(LayerShellOptions {
      anchor: Anchor::TOP,
      exclusive_zone: None,
      exclusive_edge: None,
      margin: Some((px(top), px(0.), px(0.), px(0.))),
      layer: Layer::Overlay,
      namespace: NAMESPACE.to_string(),
      // on-demand loses the keyboard to a clicked window, which is what unfocus closes on
      keyboard_interactivity: match unfocus {
        true => KeyboardInteractivity::OnDemand,
        false => KeyboardInteractivity::Exclusive,
      },
    }),
    window_background: WindowBackgroundAppearance::Transparent,
    window_decorations: Some(WindowDecorations::Client),
    app_id: Some(APP_NAME.to_string()),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: Point::default(),
      size: size(px(WIDTH), px(HEIGHT)),
    })),
    display_id: Some(display),
    ..Default::default()
  };

  let mut view = WeakEntity::new_invalid();
  let handle = cx.open_window(options, |window, cx| {
    let launcher = cx.new(|cx| Launcher::new(open, window, cx));
    view = launcher.downgrade();
    launcher.update(cx, |launcher, cx| launcher.focus(window, cx));
    cx.new(|cx| Root::new(launcher, window, cx).bg(gpui_kit::transparent_black()))
  })?;
  cx.set_global(LauncherState {
    handle: handle.into(),
    view,
  });
  Ok(())
}

pub(crate) fn window_size(preview: bool, custom: Option<Size<Pixels>>) -> Size<Pixels> {
  match (custom, preview) {
    (Some(custom), _) => custom,
    (None, true) => size(px(WIDE), px(TALL)),
    (None, false) => size(px(WIDTH), px(HEIGHT)),
  }
}
