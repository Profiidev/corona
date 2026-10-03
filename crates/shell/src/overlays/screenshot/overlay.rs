use std::{sync::Arc, time::Duration};

use corona_config::ConfigProvider;
use gpui_kit::{
  Bounds, Context, CursorStyle, DispatchPhase, Edges, Entity, FocusHandle, Hsla,
  InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
  MouseUpEvent, ParentElement, Pixels, Point, Render, RenderImage, Size, Styled, Window, canvas,
  component::{ActiveTheme, tag::Tag},
  div, img, point,
  prelude::FluentBuilder,
  px,
};

use crate::overlays::screenshot::{
  mode::Mode,
  save::{commit_selection, is_empty},
  state::{DragArea, MonitorGeometry, ScreenshotState, window_at},
  toolbar::ScreenshotToolbar,
};

const MIN_SELECTION_SIZE: f32 = 4.;
const SLIDE_ANIMATION: Duration = Duration::from_millis(100);
const DIM_ALPHA: f32 = 0.7;

fn snap(b: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
  let round = |v: Pixels| px((v.as_f32() * scale).round() / scale);
  let (x0, y0) = (round(b.left()), round(b.top()));
  let (x1, y1) = (round(b.right()), round(b.bottom()));
  Bounds {
    origin: point(x0, y0),
    size: Size::new(x1 - x0, y1 - y0),
  }
}

pub struct Overlay {
  output: String,
  geometry: MonitorGeometry,
  picture: Arc<RenderImage>,
  windows: Vec<Bounds<Pixels>>,
  cursor: Option<Point<Pixels>>,
  pub focus: FocusHandle,
  toolbar: ScreenshotToolbar,
}

impl Overlay {
  pub fn new(
    output: String,
    picture: Arc<RenderImage>,
    geometry: MonitorGeometry,
    windows: Vec<Bounds<Pixels>>,
    cx: &mut Context<'_, Self>,
  ) -> Self {
    Self {
      output,
      geometry,
      picture,
      windows,
      cursor: None,
      focus: cx.focus_handle(),
      toolbar: ScreenshotToolbar::new(),
    }
  }

  fn selection_part(&self, shown: Bounds<Pixels>) -> Option<(Bounds<Pixels>, Edges<bool>)> {
    let part = shown.intersect(&self.geometry.bounds());
    if is_empty(&part) {
      return None;
    }
    let sides = Edges {
      top: part.top() == shown.top(),
      right: part.right() == shown.right(),
      bottom: part.bottom() == shown.bottom(),
      left: part.left() == shown.left(),
    };
    let local = Bounds {
      origin: part.origin - self.geometry.origin,
      size: part.size,
    };
    Some((local, sides))
  }

  fn pointer_events(view: Entity<Overlay>) -> impl IntoElement {
    canvas(
      |_, _, _| (),
      move |_, _, window, cx| {
        if ScreenshotState::get(cx).is_some_and(|s| s.drag.is_some()) {
          window.set_window_cursor_style(CursorStyle::Crosshair);
        }

        let v = view.clone();
        window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
          if phase == DispatchPhase::Bubble {
            v.update(cx, |this, cx| this.on_move(e.position, cx));
          }
        });
        window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
          if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
            view.update(cx, |this, cx| this.on_release(window, cx));
          }
        });
      },
    )
    .absolute()
    .size_full()
  }

  fn on_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
    self.cursor = Some(position);
    let (origin, output) = (self.geometry.origin, self.output.clone());
    let under = window_at(&self.windows, position).map(|b| Bounds {
      origin: b.origin + origin,
      size: b.size,
    });
    let Some(state) = ScreenshotState::get(cx) else {
      return;
    };

    let dragging = state.drag.is_some();
    if let Some(drag) = state.drag.as_mut() {
      drag.to = position + origin;
    }
    let moved_monitor = !dragging && state.hovered_monitor != output;
    if moved_monitor {
      state.hovered_monitor = output;
    }
    let moved_window = !dragging
      && (under.is_some() || (moved_monitor && self.windows.is_empty()))
      && state.hovered_window != under;
    if moved_window {
      state.hovered_window = under;
    }
    let window_mode = state.mode == Mode::Window;
    if dragging || moved_monitor || (window_mode && moved_window) {
      ScreenshotState::refresh_all(cx);
      cx.notify();
    }
  }

  fn on_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(state) = ScreenshotState::get(cx) else {
      return;
    };
    match state.mode {
      Mode::Selection => {
        let Some(drag) = state.drag.take() else {
          return;
        };
        let area = drag.bounds();
        if area.size.width.as_f32() >= MIN_SELECTION_SIZE
          && area.size.height.as_f32() >= MIN_SELECTION_SIZE
        {
          commit_selection(area, window, cx);
        } else {
          ScreenshotState::refresh_all(cx);
          cx.notify();
        }
      }
      Mode::Monitor => commit_selection(self.geometry.bounds(), window, cx),
      Mode::Window => {
        if let Some(area) = state.hovered_window {
          commit_selection(area, window, cx);
        }
      }
    }
  }

  fn size_badge(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
    let state = cx.try_global::<ScreenshotState>()?;
    let drag = state.drag.as_ref()?;
    if state.mode != Mode::Selection || !self.geometry.bounds().contains(&drag.to) {
      return None;
    }
    let scale = state.geometry.get(&self.output)?.scale;
    let size = drag.bounds().size;
    let at = drag.to - self.geometry.origin;

    Some(
      div()
        .absolute()
        .left(at.x + px(12.))
        .top(at.y + px(12.))
        .child(Tag::secondary().rounded_full().child(format!(
          "{} × {}",
          (size.width.as_f32() * scale).round(),
          (size.height.as_f32() * scale).round()
        ))),
    )
  }
}

impl Render for Overlay {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(state) = cx.try_global::<ScreenshotState>() else {
      return div().size_full();
    };

    let cursor = match state.mode {
      Mode::Selection => CursorStyle::Crosshair,
      Mode::Monitor | Mode::Window => CursorStyle::PointingHand,
    };

    let duration = if state.mode == Mode::Selection || cx.reduce_motion() {
      Duration::ZERO
    } else {
      SLIDE_ANIMATION.mul_f32(cx.config().animation_speed)
    };
    let pill_duration = if cx.reduce_motion() {
      Duration::ZERO
    } else {
      SLIDE_ANIMATION.mul_f32(cx.config().animation_speed)
    };
    let target = state.target();
    let mode = state.mode;

    let Some(state) = ScreenshotState::get(cx) else {
      return div().size_full();
    };
    let (shown_selection, selection_moving) = state.slide.step(target, duration);
    if selection_moving {
      window.request_animation_frame();
    }

    let highlight_area = shown_selection.and_then(|b| self.selection_part(b));
    let vp = window.viewport_size();
    let theme = cx.theme();

    let accent = theme.primary;
    let dim = Hsla {
      a: DIM_ALPHA,
      ..theme.overlay
    };
    let strip = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
      div()
        .absolute()
        .left(x)
        .top(y)
        .w(w.max(px(0.)))
        .h(h.max(px(0.)))
        .bg(dim)
    };

    div()
      .track_focus(&self.focus)
      .relative()
      .size_full()
      .cursor(cursor)
      .on_key_down(cx.listener(|_, e: &KeyDownEvent, window, cx| {
        match e.keystroke.key.as_str() {
          "escape" if ScreenshotState::get(cx).is_some_and(|s| s.drag.is_some()) => {
            ScreenshotState::clear_drag(cx);
          }
          "escape" => {
            ScreenshotState::close(Some(window), cx);
          }
          "s" => ScreenshotState::set_mode(Mode::Selection, cx),
          "m" => ScreenshotState::set_mode(Mode::Monitor, cx),
          "w" => ScreenshotState::set_mode(Mode::Window, cx),
          "enter"
            if let Some(state) = ScreenshotState::get(cx)
              && state.mode == Mode::Monitor
              && let Some(g) = state.geometry.get(&state.hovered_monitor) =>
          {
            commit_selection(g.bounds(), window, cx);
          }
          _ => return,
        }
        cx.notify();
      }))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, e: &MouseDownEvent, _, cx| {
          if let Some(state) = ScreenshotState::get(cx)
            && state.mode == Mode::Selection
          {
            let at = e.position + this.geometry.origin;
            state.drag = Some(DragArea { from: at, to: at });
            cx.notify();
          }
        }),
      )
      .on_mouse_down(
        MouseButton::Right,
        cx.listener(|_, _: &MouseDownEvent, window, cx| {
          if ScreenshotState::get(cx).is_some_and(|s| s.drag.is_some()) {
            ScreenshotState::clear_drag(cx);
          } else {
            ScreenshotState::close(Some(window), cx);
          }
        }),
      )
      .child(img(self.picture.clone()).size_full())
      .child(Self::pointer_events(cx.entity()))
      .when_none(&highlight_area, |d| {
        d.child(strip(px(0.), px(0.), vp.width, vp.height))
      })
      .when_some(highlight_area, |d, (b, sides)| {
        let b = snap(b, window.scale_factor());
        let (x, y, w, h) = (b.origin.x, b.origin.y, b.size.width, b.size.height);

        d.child(strip(px(0.), px(0.), vp.width, y))
          .child(strip(px(0.), y + h, vp.width, vp.height - y - h))
          .child(strip(px(0.), y, x, h))
          .child(strip(x + w, y, vp.width - x - w, h))
          .child(
            div()
              .absolute()
              .left(x)
              .top(y)
              .w(w)
              .h(h)
              .when(sides.top, |d| d.border_t_2())
              .when(sides.right, |d| d.border_r_2())
              .when(sides.bottom, |d| d.border_b_2())
              .when(sides.left, |d| d.border_l_2())
              .border_color(accent),
          )
      })
      .when_some(self.size_badge(cx), |d, badge| d.child(badge))
      .child(self.toolbar.render(mode, pill_duration, window, cx))
  }
}
