pub mod commands;
mod view;

use anyhow::{Context as _, Result};
use corona_compositor::{Compositor, CompositorExt};
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Global, Styled, WeakEntity, WindowBackgroundAppearance,
  base::Root,
};

use crate::overlays::{
  OverlayState, fullscreen_options,
  switcher::{commands::Options, view::Switcher},
};

const NAMESPACE: &str = "corona_switcher";

pub struct SwitcherState {
  overlays: Vec<AnyWindowHandle>,
  view: WeakEntity<Switcher>,
}

impl Global for SwitcherState {}

impl OverlayState for SwitcherState {
  fn overlays(&self) -> &[AnyWindowHandle] {
    &self.overlays
  }
}

impl SwitcherState {
  fn cycle(options: Options, cx: &mut App) -> Result<()> {
    if let Some(state) = Self::get(cx)
      && let (Some(view), Some(handle)) = (state.view.upgrade(), state.overlays.first().copied())
    {
      handle.update(cx, |_, window, cx| {
        let shift = window.modifiers().shift;
        view.update(cx, |view, cx| view.step(shift, cx));
      })?;
      return Ok(());
    }

    // window positions have no event
    Compositor::refresh_windows(cx)?;
    let monitor = cx.compositor().active_monitor(cx).name.clone();
    let filter = options.current_monitor.then(|| monitor.clone());
    if view::order(options.mode, filter.as_deref(), cx).is_empty() {
      return Ok(());
    }
    let display = display_id_for(&monitor, cx).context("no display for the focused monitor")?;
    let mut window_options = fullscreen_options(NAMESPACE, display);
    window_options.window_background = WindowBackgroundAppearance::Transparent;

    let mut view = WeakEntity::new_invalid();
    let handle = cx.open_window(window_options, |window, cx| {
      let switcher = cx.new(|cx| Switcher::new(options, filter, cx));
      view = switcher.downgrade();
      let focus = switcher.read(cx).focus.clone();
      window.focus(&focus, cx);
      cx.new(|cx| Root::new(switcher, window, cx).bg(gpui_kit::transparent_black()))
    })?;
    cx.set_global(SwitcherState {
      overlays: vec![handle.into()],
      view,
    });
    Ok(())
  }
}
