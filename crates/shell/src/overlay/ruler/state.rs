use std::time::Duration;

use anyhow::{Result, bail};
use corona_capture::{capture_all, image::RgbaImage};
use corona_compositor::CompositorExt;
use corona_config::ConfigProvider;
use corona_utils::display::display_uuid;
use gpui_kit::{AnyWindowHandle, App, AppContext, Entity, Global};
use tracing::{error, warn};

use crate::overlay::{
  colorpicker::state::cursor_on,
  live::{LiveFeed, fps},
  ruler::{mode::RulerMode, overlay::Overlay},
  screenshot::state::{LAUNCHER_POLL, LAUNCHER_SETTLE, launcher_wait_over},
};
use corona_surface::overlay::{OverlayState, root, transparent_fullscreen_options};

const NAMESPACE: &str = "corona_ruler";
const TOLERANCE_STEP: u8 = 5;

pub struct RulerState {
  overlays: Vec<AnyWindowHandle>,
  views: Vec<Entity<Overlay>>,
  pub mode: RulerMode,
  pub tolerance: u8,
  pub hovered_monitor: String,
  pub live: bool,
}

impl Global for RulerState {}

impl OverlayState for RulerState {
  fn overlays(&self) -> &[AnyWindowHandle] {
    &self.overlays
  }
}

impl RulerState {
  /// Freezes every monitor after `delay` and opens the ruler over them, or over the live
  /// screen when `live`
  pub fn start(mode: RulerMode, delay: Duration, live: bool, cx: &mut App) {
    if cx.has_global::<Self>() {
      return;
    }
    cx.set_global(RulerState {
      overlays: Vec::new(),
      views: Vec::new(),
      mode,
      tolerance: cx.config().ruler.tolerance,
      hovered_monitor: cx.compositor().active_monitor(cx).name.clone(),
      live,
    });

    let names: Vec<String> = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .filter(|m| !m.disabled)
      .map(|m| m.name.clone())
      .collect();

    // started from the launcher: its surface must be unmapped before the frames are taken
    let from_launcher = corona_launcher::ui::is_open(cx);

    cx.spawn(async move |cx| {
      cx.background_executor().timer(delay).await;
      if from_launcher {
        let mut waited = Duration::ZERO;
        while !launcher_wait_over(cx.update(|cx| corona_launcher::ui::is_open(cx)), waited) {
          cx.background_executor().timer(LAUNCHER_POLL).await;
          waited += LAUNCHER_POLL;
        }
        cx.background_executor().timer(LAUNCHER_SETTLE).await;
      }
      let frozen = cx
        .background_executor()
        .spawn(async move {
          if live {
            return Ok(
              names
                .into_iter()
                .map(|n| (n, RgbaImage::new(0, 0)))
                .collect(),
            );
          }
          capture_all(names)?
            .into_iter()
            .map(|(name, frame)| Ok((name, frame.read_all()?)))
            .collect::<Result<Vec<_>>>()
        })
        .await;

      cx.update(|cx| {
        if let Err(e) = frozen.and_then(|f| Self::open(f, cx)) {
          error!("ruler overlay failed: {e:#}");
          Self::close(None, cx);
        }
      });
    })
    .detach();
  }

  /// One overlay per monitor; empty images stream their monitor live
  fn open(frozen: Vec<(String, RgbaImage)>, cx: &mut App) -> Result<()> {
    let displays = cx.displays();
    let cap = cx.config().ruler.live_fps;
    let monitors = cx.compositor().list_monitors(cx).to_vec();
    let cursor = cx
      .compositor()
      .cursor_position()
      .inspect_err(|e| warn!("ruler: no cursor position: {e:#}"))
      .ok();

    for (name, image) in frozen {
      let uuid = display_uuid(&name);
      let (Some(display), Some(monitor)) = (
        displays.iter().find(|d| d.uuid().ok() == Some(uuid)),
        monitors.iter().find(|m| m.name == name),
      ) else {
        continue;
      };

      let scale = monitor.scale.max(0.1);
      let live = image.width() == 0;
      let size = match live {
        true => (monitor.width, monitor.height),
        false => image.dimensions(),
      };
      let local = cursor.and_then(|c| cursor_on(c, (monitor.x, monitor.y), size, scale));
      if local.is_some()
        && let Some(state) = Self::get(cx)
      {
        state.hovered_monitor = name.clone();
      }

      let fps = fps(monitor.refresh_rate, cap);
      let mut view = None;
      let handle = cx.open_window(
        transparent_fullscreen_options(NAMESPACE, display.id()),
        |window, cx| {
          let overlay = cx.new(|cx| {
            let mut overlay = Overlay::new(name, image, scale, local, fps, cx);
            if live {
              LiveFeed::start(&mut overlay, fps, false, cx);
            }
            overlay
          });
          let focus = overlay.read(cx).focus.clone();
          window.focus(&focus, cx);
          view = Some(overlay.clone());
          root(overlay, window, cx)
        },
      )?;

      let Some(state) = Self::get(cx) else {
        bail!("RulerState was removed while opening overlay");
      };
      state.overlays.push(handle.into());
      state.views.extend(view);
    }

    if Self::get(cx).is_none_or(|s| s.overlays.is_empty()) {
      bail!("no display matched a captured monitor");
    }
    Ok(())
  }

  pub fn set_mode(mode: RulerMode, cx: &mut App) {
    if let Some(state) = Self::get(cx) {
      state.mode = mode;
      Self::refresh_all(cx);
    }
  }

  /// Switches every overlay between the live screen and a frozen capture
  pub fn toggle_live(cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    state.live = !state.live;
    let (live, views) = (state.live, state.views.clone());
    // called from a key or click handler of one of the views, which is being updated
    cx.defer(move |cx| {
      for view in views {
        view.update(cx, |v, cx| v.set_live(live, cx));
      }
      Self::refresh_all(cx);
    });
  }

  /// Raises the tolerance by `steps` steps, lowers it when negative
  pub fn adjust_tolerance(steps: i32, cx: &mut App) {
    if let Some(state) = Self::get(cx) {
      state.tolerance = stepped(state.tolerance, steps);
      Self::refresh_all(cx);
    }
  }
}

#[cfg(test)]
impl RulerState {
  /// A state without overlays, on `hovered_monitor`
  pub(crate) fn test(mode: RulerMode, tolerance: u8, hovered_monitor: &str) -> Self {
    Self {
      overlays: Vec::new(),
      views: Vec::new(),
      mode,
      tolerance,
      hovered_monitor: hovered_monitor.into(),
      live: false,
    }
  }
}

fn stepped(tolerance: u8, steps: i32) -> u8 {
  (i32::from(tolerance) + steps * i32::from(TOLERANCE_STEP)).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
  use super::stepped;

  #[test]
  fn tolerance_steps_and_clamps() {
    assert_eq!(stepped(30, 1), 35);
    assert_eq!(stepped(30, -2), 20);
    assert_eq!(stepped(3, -1), 0);
    assert_eq!(stepped(253, 1), 255);
  }
}
