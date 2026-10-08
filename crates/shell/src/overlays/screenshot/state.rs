use std::collections::HashMap;

use anyhow::{Result, bail};
use corona_capture::{Frame, capture_all};
use corona_components::animation::bounds::BoundsAnimation;
use corona_compositor::{Compositor, CompositorExt};
use corona_utils::display::display_uuid;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Global, Pixels, Point, Size, base::Root, point, px,
};
use tracing::{error, warn};

use crate::overlays::{
  OverlayState, fullscreen_options,
  screenshot::{mode::Mode, overlay::Overlay},
};

const NAMESPACE: &str = "corona_screenshot";

pub struct ScreenshotState {
  overlays: Vec<AnyWindowHandle>,
  pub screenshots: HashMap<String, Frame>,
  pub geometry: HashMap<String, MonitorGeometry>,
  pub mode: Mode,
  pub hovered_monitor: String,
  pub hovered_window: Option<Bounds<Pixels>>,
  pub windows: Vec<(String, Bounds<Pixels>)>,
  pub keyboard: bool,
  pub drag: Option<DragArea>,
  pub slide: BoundsAnimation,
  pub finishing: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
  Left,
  Right,
  Up,
  Down,
}

impl Direction {
  pub fn from_key(key: &str) -> Option<Self> {
    match key {
      "left" | "h" => Some(Self::Left),
      "right" | "l" => Some(Self::Right),
      "up" | "k" => Some(Self::Up),
      "down" | "j" => Some(Self::Down),
      _ => None,
    }
  }
}

fn nearest<K>(
  from: Bounds<Pixels>,
  dir: Direction,
  candidates: impl IntoIterator<Item = (K, Bounds<Pixels>)>,
) -> Option<(K, Bounds<Pixels>)> {
  let from = from.center();
  candidates
    .into_iter()
    .filter_map(|(key, b)| {
      let d = b.center() - from;
      let (dx, dy) = (d.x.as_f32(), d.y.as_f32());
      let (ahead, aside) = match dir {
        Direction::Left => (-dx, dy),
        Direction::Right => (dx, dy),
        Direction::Up => (-dy, dx),
        Direction::Down => (dy, dx),
      };
      (ahead > 0.).then_some((ahead + 2. * aside.abs(), key, b))
    })
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, key, b)| (key, b))
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

impl Global for ScreenshotState {}

impl OverlayState for ScreenshotState {
  fn overlays(&self) -> &[AnyWindowHandle] {
    &self.overlays
  }
}

pub fn window_at(windows: &[Bounds<Pixels>], p: Point<Pixels>) -> Option<Bounds<Pixels>> {
  windows.iter().find(|b| b.contains(&p)).copied()
}

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
      windows: Vec::new(),
      keyboard: false,
      drag: None,
      slide: BoundsAnimation::default(),
      finishing: false,
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

  fn open(frozen: Vec<(String, Frame)>, cx: &mut App) -> Result<()> {
    let displays = cx.displays();
    let monitors = cx.compositor().list_monitors(cx).to_vec();
    let all_windows = cx.compositor().list_windows(cx).to_vec();
    let cursor = cx
      .compositor()
      .cursor_position()
      .map(|(x, y)| point(px(x as f32), px(y as f32)))
      .inspect_err(|e| warn!("screenshot: no cursor position: {e:#}"))
      .ok();
    let mut hovered = None;

    for (name, image) in frozen {
      let uuid = display_uuid(&name);
      let (Some(display), Some(monitor)) = (
        displays.iter().find(|d| d.uuid().ok() == Some(uuid)),
        monitors.iter().find(|m| m.name == name),
      ) else {
        continue;
      };

      let mut visible = all_windows
        .iter()
        .filter(|w| {
          w.monitor == monitor.id
            && (w.workspace == monitor.active_workspace.id
              || monitor
                .active_scratchpad
                .as_ref()
                .is_some_and(|s| s.id == w.workspace))
            && !w.hidden
            && w.width > 0
            && w.height > 0
        })
        .collect::<Vec<_>>();
      visible.sort_by_key(|w| std::cmp::Reverse(w.stacking()));
      let visible = visible
        .into_iter()
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

      if let Some(cursor) = cursor
        && geometry.bounds().contains(&cursor)
      {
        let window = window_at(&visible, cursor - geometry.origin).map(|b| Bounds {
          origin: b.origin + geometry.origin,
          size: b.size,
        });
        hovered = Some((name.clone(), window));
      }

      let global_windows = visible
        .iter()
        .map(|b| {
          let global = Bounds {
            origin: b.origin + geometry.origin,
            size: b.size,
          };
          (name.clone(), global)
        })
        .collect::<Vec<_>>();

      let picture = image.surface.clone();
      let handle = cx.open_window(fullscreen_options(NAMESPACE, display.id()), |window, cx| {
        let view = cx.new(|cx| Overlay::new(name.clone(), picture, geometry, visible, cx));
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.new(|cx| Root::new(view, window, cx))
      })?;

      let Some(state) = Self::get(cx) else {
        bail!("ScreenshotState was removed while opening overlay");
      };
      state.overlays.push(handle.into());
      state.windows.extend(global_windows);
      state.screenshots.insert(name.clone(), image);
      state.geometry.insert(name, geometry);
    }

    let Some(state) = Self::get(cx) else {
      bail!("ScreenshotState was removed while opening overlay");
    };
    if state.overlays.is_empty() {
      bail!("no display matched a captured monitor");
    }
    if let Some((monitor, window)) = hovered {
      state.hovered_monitor = monitor;
      state.hovered_window = window;
    }

    Ok(())
  }

  pub fn set_mode(mode: Mode, cx: &mut App) {
    let Some(state) = Self::get(cx).filter(|s| !s.finishing) else {
      return;
    };
    state.mode = mode;
    state.drag = None;
    Self::refresh_all(cx);
  }

  pub fn navigate(dir: Direction, cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    let monitor = state
      .geometry
      .get(&state.hovered_monitor)
      .map(MonitorGeometry::bounds);
    let Some(from) = state.target().or(monitor) else {
      return;
    };

    match state.mode {
      Mode::Selection => return,
      Mode::Monitor => {
        let monitors = state.geometry.iter().map(|(n, g)| (n, g.bounds()));
        let Some((name, _)) = nearest(from, dir, monitors) else {
          return;
        };
        state.hovered_monitor = name.clone();
      }
      Mode::Window => {
        let windows = state.windows.iter().map(|(n, b)| (n, *b));
        let Some((name, window)) = nearest(from, dir, windows) else {
          return;
        };
        state.hovered_monitor = name.clone();
        state.hovered_window = Some(window);
      }
    }
    state.keyboard = true;
    Self::refresh_all(cx);
  }

  pub fn clear_drag(cx: &mut App) {
    let Some(state) = Self::get(cx) else {
      return;
    };
    state.drag = None;
    Self::refresh_all(cx);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(x: f32, y: f32) -> Bounds<Pixels> {
    Bounds {
      origin: point(px(x), px(y)),
      size: Size::new(px(100.), px(100.)),
    }
  }

  #[test]
  fn nearest_prefers_straight_ahead() {
    let from = rect(0., 0.);
    let candidates = [
      ("ahead", rect(400., 0.)),
      ("closer but aside", rect(150., 300.)),
      ("behind", rect(-200., 0.)),
    ];

    let pick = |dir| nearest(from, dir, candidates).map(|(k, _)| k);
    assert_eq!(pick(Direction::Right), Some("ahead"));
    assert_eq!(pick(Direction::Left), Some("behind"));
    assert_eq!(pick(Direction::Down), Some("closer but aside"));
    assert_eq!(pick(Direction::Up), None);
  }

  fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
      origin: point(px(x), px(y)),
      size: Size::new(px(w), px(h)),
    }
  }

  #[test]
  fn direction_from_key() {
    for (keys, dir) in [
      (["left", "h"], Direction::Left),
      (["right", "l"], Direction::Right),
      (["up", "k"], Direction::Up),
      (["down", "j"], Direction::Down),
    ] {
      for key in keys {
        assert_eq!(Direction::from_key(key), Some(dir), "{key}");
      }
    }
    for key in ["", "H", "Left", "x", "space"] {
      assert_eq!(Direction::from_key(key), None, "{key}");
    }
  }

  #[test]
  fn drag_bounds_normalized_for_every_direction() {
    let corners = [(10., 20.), (110., 20.), (10., 70.), (110., 70.)];
    for from in corners {
      for to in corners {
        if from.0 == to.0 || from.1 == to.1 {
          continue;
        }
        let drag = DragArea {
          from: point(px(from.0), px(from.1)),
          to: point(px(to.0), px(to.1)),
        };
        assert_eq!(drag.bounds(), bounds(10., 20., 100., 50.));
      }
    }
  }

  #[test]
  fn drag_bounds_zero_size() {
    let p = point(px(5.), px(7.));
    let b = DragArea { from: p, to: p }.bounds();
    assert_eq!(b.origin, p);
    assert_eq!(b.size, Size::new(px(0.), px(0.)));
  }

  #[test]
  fn window_at_first_match_wins() {
    let top = bounds(0., 0., 100., 100.);
    let below = bounds(50., 50., 100., 100.);
    let windows = [top, below];
    assert_eq!(window_at(&windows, point(px(60.), px(60.))), Some(top));
    assert_eq!(window_at(&windows, point(px(120.), px(120.))), Some(below));
    assert_eq!(window_at(&windows, point(px(300.), px(0.))), None);
    assert_eq!(window_at(&[], point(px(0.), px(0.))), None);
  }

  #[test]
  fn window_at_edges() {
    let w = bounds(0., 0., 100., 100.);
    assert_eq!(window_at(&[w], point(px(0.), px(0.))), Some(w));
    assert_eq!(window_at(&[w], point(px(99.9), px(99.9))), Some(w));
    // the far edge is exclusive
    assert_eq!(window_at(&[w], point(px(100.), px(50.))), None);
    assert_eq!(window_at(&[w], point(px(50.), px(100.))), None);
  }

  #[test]
  fn nearest_empty_and_self() {
    let from = rect(0., 0.);
    let none: [(u8, Bounds<Pixels>); 0] = [];
    assert!(nearest(from, Direction::Right, none).is_none());
    // a candidate at the same center is never "ahead"
    for dir in [
      Direction::Left,
      Direction::Right,
      Direction::Up,
      Direction::Down,
    ] {
      assert!(nearest(from, dir, [(0, from)]).is_none());
    }
  }

  #[test]
  fn nearest_tie_keeps_first() {
    let from = rect(0., 0.);
    let candidates = [("a", rect(200., 0.)), ("b", rect(200., 0.))];
    assert_eq!(
      nearest(from, Direction::Right, candidates).map(|(k, _)| k),
      Some("a")
    );
  }

  #[test]
  fn nearest_weights_offset_twice() {
    let from = rect(0., 0.);
    // score: 100 + 2*100 = 300 vs 290 + 0
    let candidates = [("diagonal", rect(100., 100.)), ("straight", rect(290., 0.))];
    assert_eq!(
      nearest(from, Direction::Right, candidates).map(|(k, _)| k),
      Some("straight")
    );
    let candidates = [("diagonal", rect(100., 100.)), ("straight", rect(310., 0.))];
    assert_eq!(
      nearest(from, Direction::Right, candidates).map(|(k, _)| k),
      Some("diagonal")
    );
    // the sign of the offset does not matter
    let candidates = [("above", rect(100., -100.)), ("straight", rect(310., 0.))];
    assert_eq!(
      nearest(from, Direction::Right, candidates).map(|(k, _)| k),
      Some("above")
    );
  }

  #[test]
  fn nearest_up_and_left() {
    let from = rect(0., 0.);
    let candidates = [("up", rect(0., -300.)), ("left", rect(-300., 0.))];
    let pick = |dir| nearest(from, dir, candidates).map(|(k, _)| k);
    assert_eq!(pick(Direction::Up), Some("up"));
    assert_eq!(pick(Direction::Left), Some("left"));
    assert_eq!(pick(Direction::Down), None);
    assert_eq!(pick(Direction::Right), None);
  }

  #[test]
  fn monitor_geometry_bounds() {
    let g = MonitorGeometry {
      origin: point(px(1920.), px(0.)),
      size: Size::new(px(1280.), px(720.)),
      scale: 1.5,
    };
    assert_eq!(g.bounds(), bounds(1920., 0., 1280., 720.));
  }
}
