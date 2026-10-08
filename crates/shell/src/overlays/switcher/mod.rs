pub mod commands;
mod view;

use anyhow::{Context as _, Result};
use corona_compositor::{Compositor, CompositorExt};
use corona_config::ConfigProvider;
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
    let current_monitor =
      options.current_monitor || cx.config().window_switcher.current_monitor_only;
    let filter = current_monitor.then(|| monitor.clone());
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    overlays::{switcher::commands::Mode, taskbar::tests::window},
    test_support::{FakeCompositor, setup, workspace},
  };
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  fn nothing_to_switch_opens_nothing(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    for mode in [Mode::Window, Mode::Workspace] {
      let options = Options {
        mode,
        ..Default::default()
      };
      cx.update(|cx| SwitcherState::cycle(options, cx)).unwrap();
      assert!(!cx.update(|cx| cx.has_global::<SwitcherState>()));
    }
  }

  #[gpui::test]
  fn other_monitor_only_counts_without_filter(cx: &mut TestAppContext) {
    // the focused monitor is DP-1, the only window is on HDMI-A-1
    setup(
      FakeCompositor {
        workspaces: vec![workspace("2", "HDMI-A-1")],
        windows: vec![gpui_window("2")],
        ..Default::default()
      },
      cx,
    );
    let options = Options {
      current_monitor: true,
      ..Default::default()
    };
    cx.update(|cx| SwitcherState::cycle(options, cx)).unwrap();
    assert!(!cx.update(|cx| cx.has_global::<SwitcherState>()));
    // without the filter there is a window, but no display for DP-1 in tests
    assert!(
      cx.update(|cx| SwitcherState::cycle(Options::default(), cx))
        .is_err()
    );
    assert!(!cx.update(|cx| cx.has_global::<SwitcherState>()));
  }

  fn gpui_window(workspace: &str) -> corona_compositor::types::Window {
    corona_compositor::types::Window {
      workspace: workspace.into(),
      ..window("0x1", "kitty", 100, 100)
    }
  }
}
