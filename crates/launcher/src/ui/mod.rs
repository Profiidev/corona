mod form;
pub mod hud;
mod launcher;
mod overlays;
mod rows;
pub mod style;

use anyhow::Result;
use corona_config::ConfigProvider;
use corona_surface::overlay::open_on_focused;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Global, KeyBinding, Pixels, Size, WeakEntity, px, size,
};

pub use launcher::Launcher;

use crate::view::View;

const NAMESPACE: &str = "corona_launcher";
const CONTEXT: &str = "Launcher";
/// Smaller would cut the search bar and footer
const MIN_WIDTH: f32 = 400.;
const MIN_HEIGHT: f32 = 240.;

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
    ToggleActions,
    TogglePreview,
    ToggleFilter,
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
      KeyBinding::new("ctrl-f", ToggleFilter, c),
      KeyBinding::new("ctrl-s", Submit, c),
    ]);
  }
}

/// How the launcher opens
pub enum Open {
  /// The root search, optionally with a query
  Root { query: String },
  /// Straight into a command, as a per-command keybind does
  Command {
    id: String,
    query: String,
    /// Run its first action; otherwise the root search shows it for the query
    run: bool,
  },
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
    // a surface destroyed from outside leaves a dead state behind: open anew then
    if cx.global::<LauncherState>().view.upgrade().is_some() {
      close(cx);
      return Ok(());
    }
    cx.remove_global::<LauncherState>();
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

  // a full screen surface with the card centred: the keyboard stays with it wherever the
  // pointer is, and a click beside the card is what closes it
  let mut view = WeakEntity::new_invalid();
  let handle = open_on_focused(
    NAMESPACE,
    |_| {},
    cx,
    |window, cx| {
      let launcher = cx.new(|cx| Launcher::new(open, window, cx));
      view = launcher.downgrade();
      launcher.update(cx, |launcher, cx| launcher.focus(window, cx));
      launcher
    },
  )?;
  cx.set_global(LauncherState { handle, view });
  Ok(())
}

/// Every view has the same card, as large as the config says
pub(crate) fn window_size(cx: &App) -> Size<Pixels> {
  let launcher = &cx.config().launcher;
  size(
    px(launcher.width.max(MIN_WIDTH)),
    px(launcher.height.max(MIN_HEIGHT)),
  )
}
