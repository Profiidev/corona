use anyhow::{Result, bail};
use corona_capture::{capture_all, image::RgbaImage};
use corona_compositor::CompositorExt;
use corona_utils::display::display_uuid;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, ClipboardItem, Global, Window, base::Root, point, px,
};
use tracing::{error, warn};

use crate::overlays::{colorpicker::overlay::Overlay, fullscreen_options};

const NAMESPACE: &str = "corona_colorpicker";

pub struct ColorPickerState {
  overlays: Vec<AnyWindowHandle>,
  pub hovered_monitor: String,
}

impl Global for ColorPickerState {}

impl ColorPickerState {
  pub fn capture(cx: &mut App) {
    if cx.has_global::<Self>() {
      return;
    }
    cx.set_global(ColorPickerState {
      overlays: Vec::new(),
      hovered_monitor: cx.compositor().active_monitor(cx).name.clone(),
    });

    let names: Vec<String> = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .filter(|m| !m.disabled)
      .map(|m| m.name.clone())
      .collect();

    cx.spawn(async move |cx| {
      let frozen = cx
        .background_executor()
        .spawn(async move { capture_all(names) })
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

  fn open(frozen: Vec<(String, RgbaImage)>, cx: &mut App) -> Result<()> {
    let displays = cx.displays();
    let monitors = cx.compositor().list_monitors(cx).to_vec();
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
      let (w, h) = (image.width() as f32 / scale, image.height() as f32 / scale);
      let local = cursor
        .map(|(x, y)| ((x - monitor.x) as f32, (y - monitor.y) as f32))
        .filter(|&(x, y)| (0. ..w).contains(&x) && (0. ..h).contains(&y))
        .map(|(x, y)| point(px(x), px(y)));
      if local.is_some()
        && let Some(state) = Self::get(cx)
      {
        state.hovered_monitor = name.clone();
      }

      let handle = cx.open_window(fullscreen_options(NAMESPACE, display.id()), |window, cx| {
        let view = cx.new(|cx| Overlay::new(name, image, scale, local, cx));
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.new(|cx| Root::new(view, window, cx))
      })?;

      let Some(state) = Self::get(cx) else {
        bail!("ColorPickerState was removed while opening overlay");
      };
      state.overlays.push(handle.into());
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

  pub fn close(current: Option<&mut Window>, cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    let state = cx.remove_global::<Self>();

    for handle in &state.overlays {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    if let Some(window) = current {
      window.remove_window();
    }
  }

  pub fn refresh_all(cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    for handle in state.overlays.clone() {
      let _ = handle.update(cx, |_, window, _| window.refresh());
    }
  }

  pub fn get(cx: &mut App) -> Option<&mut Self> {
    if cx.has_global::<Self>() {
      Some(cx.global_mut::<Self>())
    } else {
      None
    }
  }
}
