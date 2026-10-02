use std::{
  collections::HashMap,
  time::{Duration, Instant},
};

use anyhow::{Result, bail};
use corona_capture::{RgbaImageExt, capture_all, image::RgbaImage};
use corona_compositor::{Compositor, CompositorExt};
use corona_utils::display::display_uuid;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Global, Pixels, Point, Size, Window,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  base::{Root, animation::ease_out_cubic},
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::{error, warn};

use crate::overlays::screenshot::{mode::Mode, overlay::Overlay};

const NAMESPACE: &str = "corona_screenshot";

pub struct ScreenshotState {
  overlays: Vec<AnyWindowHandle>,
  pub screenshots: HashMap<String, RgbaImage>,
  pub geometry: HashMap<String, MonitorGeometry>,
  pub mode: Mode,
  pub hovered_monitor: String,
  pub hovered_window: Option<Bounds<Pixels>>,
  pub drag: Option<DragArea>,
  pub slide: AreaSlideAnimation,
}

#[derive(Clone, Copy)]
pub struct MonitorGeometry {
  pub origin: Point<Pixels>,
  pub size: Size<Pixels>,
  pub scale: f32,
}

impl MonitorGeometry {
  pub fn bounds(&self) -> Bounds<Pixels> {
    Bounds {
      origin: self.origin,
      size: self.size,
    }
  }
}

pub struct DragArea {
  pub from: Point<Pixels>,
  pub to: Point<Pixels>,
}

impl DragArea {
  pub fn bounds(&self) -> Bounds<Pixels> {
    Bounds {
      origin: point(self.from.x.min(self.to.x), self.from.y.min(self.to.y)),
      size: Size::new(
        (self.from.x - self.to.x).abs(),
        (self.from.y - self.to.y).abs(),
      ),
    }
  }
}

#[derive(Default)]
pub struct AreaSlideAnimation {
  target: Option<Bounds<Pixels>>,
  shown: Option<Bounds<Pixels>>,
  from: Option<(Bounds<Pixels>, Instant)>,
}

impl AreaSlideAnimation {
  pub fn step(
    &mut self,
    target: Option<Bounds<Pixels>>,
    duration: Duration,
  ) -> (Option<Bounds<Pixels>>, bool) {
    if target != self.target {
      self.from = match (self.shown, target) {
        (Some(shown), Some(_)) if !duration.is_zero() => Some((shown, Instant::now())),
        _ => None,
      };
      self.target = target;
    }

    let (shown, moving) = match (self.target, self.from) {
      (Some(to), Some((from, start))) => {
        let t = (start.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.);
        let e = ease_out_cubic(t);
        let lerp = |a: Pixels, b: Pixels| px(a.as_f32() + (b - a).as_f32() * e);
        let b = Bounds {
          origin: point(
            lerp(from.origin.x, to.origin.x),
            lerp(from.origin.y, to.origin.y),
          ),
          size: Size::new(
            lerp(from.size.width, to.size.width),
            lerp(from.size.height, to.size.height),
          ),
        };
        (Some(b), t < 1.)
      }
      (to, _) => (to, false),
    };
    self.shown = shown;
    (shown, moving)
  }
}

impl Global for ScreenshotState {}

impl ScreenshotState {
  pub fn target(&self) -> Option<Bounds<Pixels>> {
    match self.mode {
      Mode::Selection => self.drag.as_ref().map(DragArea::bounds),
      Mode::Monitor => self
        .geometry
        .get(&self.hovered_monitor)
        .map(MonitorGeometry::bounds),
      Mode::Window => self.hovered_window,
    }
  }

  pub fn capture(mode: Mode, cx: &mut App) {
    if cx.has_global::<Self>() {
      return;
    }
    if let Err(e) = Compositor::refresh_windows(cx) {
      warn!("screenshot: window list may be stale: {e:#}");
    }

    cx.set_global(ScreenshotState {
      overlays: Vec::new(),
      screenshots: HashMap::new(),
      geometry: HashMap::new(),
      mode,
      hovered_monitor: cx.compositor().active_monitor(cx).name.clone(),
      hovered_window: None,
      drag: None,
      slide: AreaSlideAnimation::default(),
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
          error!("screenshot overlay failed: {e:#}");
          Self::close(None, cx);
        }
      });
    })
    .detach();
  }

  fn open(frozen: Vec<(String, RgbaImage)>, cx: &mut App) -> Result<()> {
    let displays = cx.displays();
    let monitors = cx.compositor().list_monitors(cx).to_vec();
    let all_windows = cx.compositor().list_windows(cx).to_vec();

    for (name, image) in frozen {
      let uuid = display_uuid(&name);
      let (Some(display), Some(monitor)) = (
        displays.iter().find(|d| d.uuid().ok() == Some(uuid)),
        monitors.iter().find(|m| m.name == name),
      ) else {
        continue;
      };

      let visible = all_windows
        .iter()
        .filter(|w| {
          w.monitor == monitor.id
            && (w.workspace == monitor.active_workspace.id
              || monitor
                .active_scratchpad
                .as_ref()
                .is_some_and(|s| s.id == w.workspace))
            && w.width > 0
            && w.height > 0
        })
        .map(|w| Bounds {
          origin: point(px((w.x - monitor.x) as f32), px((w.y - monitor.y) as f32)),
          size: Size::new(px(w.width as f32), px(w.height as f32)),
        })
        .collect::<Vec<_>>();

      let scale = monitor.scale.max(0.1);
      let geometry = MonitorGeometry {
        origin: point(px(monitor.x as f32), px(monitor.y as f32)),
        size: Size::new(
          px(image.width() as f32 / scale),
          px(image.height() as f32 / scale),
        ),
        scale,
      };

      let picture = image.to_gpui();
      let handle = cx.open_window(
        WindowOptions {
          kind: WindowKind::LayerShell(LayerShellOptions {
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
            exclusive_zone: Some(px(-1.)),
            exclusive_edge: None,
            margin: None,
            layer: Layer::Overlay,
            namespace: NAMESPACE.to_string(),
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
          }),
          window_background: WindowBackgroundAppearance::Opaque,
          display_id: Some(display.id()),
          titlebar: None,
          window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: Size::new(px(0.), px(0.)),
          })),
          ..Default::default()
        },
        |window, cx| {
          let view = cx.new(|cx| Overlay::new(name.clone(), picture, geometry, visible, cx));
          let focus = view.read(cx).focus.clone();
          window.focus(&focus, cx);
          cx.new(|cx| Root::new(view, window, cx))
        },
      )?;

      let Some(state) = Self::get(cx) else {
        bail!("ScreenshotState was removed while opening overlay");
      };
      state.overlays.push(handle.into());
      state.screenshots.insert(name.clone(), image);
      state.geometry.insert(name, geometry);
    }

    Ok(())
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

  pub fn set_mode(mode: Mode, cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    state.mode = mode;
    state.drag = None;
    Self::refresh_all(cx);
  }

  pub fn clear_drag(cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    state.drag = None;
    Self::refresh_all(cx);
  }

  pub fn get(cx: &mut App) -> Option<&mut Self> {
    if cx.has_global::<Self>() {
      Some(cx.global_mut::<Self>())
    } else {
      None
    }
  }
}
