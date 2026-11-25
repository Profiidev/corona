use anyhow::{Result, bail};
use corona_capture::{capture_all, image::RgbaImage};
use corona_compositor::CompositorExt;
use corona_config::ConfigProvider;
use corona_utils::display::display_uuid;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, ClipboardItem, Entity, Global, Pixels, Point, Window, point, px,
};
use tracing::{error, warn};

use crate::overlay::{
  colorpicker::overlay::Overlay,
  live::{LiveFeed, fps},
};
use corona_surface::overlay::{OverlayState, root, transparent_fullscreen_options};

const NAMESPACE: &str = "corona_colorpicker";

pub struct ColorPickerState {
  overlays: Vec<AnyWindowHandle>,
  views: Vec<Entity<Overlay>>,
  pub hovered_monitor: String,
  pub live: bool,
}

impl Global for ColorPickerState {}

impl OverlayState for ColorPickerState {
  fn overlays(&self) -> &[AnyWindowHandle] {
    &self.overlays
  }
}

impl ColorPickerState {
  /// Opens the picker over a frozen capture, or over the live screen when `live`
  pub fn capture(live: bool, cx: &mut App) {
    if cx.has_global::<Self>() {
      return;
    }
    cx.set_global(ColorPickerState {
      overlays: Vec::new(),
      views: Vec::new(),
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

    if live {
      let blank = names
        .into_iter()
        .map(|n| (n, RgbaImage::new(0, 0)))
        .collect();
      if let Err(e) = Self::open(blank, cx) {
        error!("color picker overlay failed: {e:#}");
        Self::close(None, cx);
      }
      return;
    }

    cx.spawn(async move |cx| {
      let frozen = cx
        .background_executor()
        .spawn(async move {
          capture_all(names)?
            .into_iter()
            .map(|(name, frame)| Ok((name, frame.read_all()?)))
            .collect::<Result<Vec<_>>>()
        })
        .await;

      cx.update(|cx| {
        if let Err(e) = frozen.and_then(|f| Self::open(f, cx)) {
          error!("color picker overlay failed: {e:#}");
          Self::close(None, cx);
        }
      });
    })
    .detach();
  }

  /// One overlay per monitor; empty images stream their monitor live
  fn open(frozen: Vec<(String, RgbaImage)>, cx: &mut App) -> Result<()> {
    let displays = cx.displays();
    let monitors = cx.compositor().list_monitors(cx).to_vec();
    let cap = cx.config().color_picker.live_fps;
    let cursor = cx
      .compositor()
      .cursor_position()
      .inspect_err(|e| warn!("color picker: no cursor position: {e:#}"))
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
      let origin = (monitor.x, monitor.y);
      let live = image.width() == 0;
      let size = match live {
        true => (monitor.width, monitor.height),
        false => image.dimensions(),
      };
      let local = cursor.and_then(|c| cursor_on(c, origin, size, scale));
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
        bail!("ColorPickerState was removed while opening overlay");
      };
      state.overlays.push(handle.into());
      state.views.extend(view);
    }

    if Self::get(cx).is_none_or(|s| s.overlays.is_empty()) {
      bail!("no display matched a captured monitor");
    }
    Ok(())
  }

  pub fn pick(hex: String, window: &mut Window, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(hex));
    Self::close(Some(window), cx);
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
}

/// The global `cursor` relative to a monitor at `origin` whose frame has `size` device
/// pixels; none when it is elsewhere
pub(crate) fn cursor_on(
  cursor: (i32, i32),
  origin: (i32, i32),
  size: (u32, u32),
  scale: f32,
) -> Option<Point<Pixels>> {
  let (w, h) = (size.0 as f32 / scale, size.1 as f32 / scale);
  let (x, y) = ((cursor.0 - origin.0) as f32, (cursor.1 - origin.1) as f32);
  ((0. ..w).contains(&x) && (0. ..h).contains(&y)).then(|| point(px(x), px(y)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cursor_inside_is_local() {
    let p = cursor_on((1930, 15), (1920, 0), (2560, 1440), 1.).unwrap();
    assert_eq!(p, point(px(10.), px(15.)));
    assert_eq!(
      cursor_on((1920, 0), (1920, 0), (10, 10), 1.),
      Some(point(px(0.), px(0.)))
    );
  }

  #[test]
  fn cursor_outside_is_none() {
    let size = (100, 100);
    for c in [(-1, 0), (0, -1), (100, 0), (0, 100), (500, 500)] {
      assert_eq!(cursor_on(c, (0, 0), size, 1.), None, "{c:?}");
    }
    assert_eq!(cursor_on((0, 0), (0, 0), (0, 0), 1.), None);
  }

  #[test]
  fn scale_shrinks_logical_area() {
    // 200 device pixels at scale 2 are 100 logical ones
    assert!(cursor_on((99, 99), (0, 0), (200, 200), 2.).is_some());
    assert!(cursor_on((100, 50), (0, 0), (200, 200), 2.).is_none());
  }
}
