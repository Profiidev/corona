use std::sync::Arc;

use corona_capture::{RgbaImageExt, image::RgbaImage};
use corona_components::animation::animation_duration;
use gpui_kit::{
  AnyElement, Context, CursorStyle, FocusHandle, Hsla, InteractiveElement, IntoElement,
  KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
  Point, Render, RenderImage, ScrollEvent, Styled, Window,
  component::{ActiveTheme, tag::Tag},
  div, img, px,
};

use crate::overlay::ruler::{
  measure::{Measurement, Span},
  mode::RulerMode,
  state::RulerState,
  toolbar::{RulerToolbar, SLIDE_ANIMATION},
};
use corona_surface::overlay::OverlayState;

/// How close (logical px) a right click must be to a pin to remove it
const HIT_SLACK: f32 = 6.;
/// Length of the caps at the ends of a line
const CAP: f32 = 10.;
const LABEL_OFFSET: f32 = 12.;
const BOX_FILL_ALPHA: f32 = 0.12;

pub struct Overlay {
  output: String,
  image: RgbaImage,
  scale: f32,
  picture: Arc<RenderImage>,
  cursor: Option<Point<Pixels>>,
  /// The device pixel corner a bounds drag started at
  drag_from: Option<(u32, u32)>,
  pins: Vec<Measurement>,
  /// Scrolled pixels not yet turned into tolerance steps
  scrolled: f32,
  pub focus: FocusHandle,
  toolbar: RulerToolbar,
}

impl Overlay {
  pub fn new(
    output: String,
    image: RgbaImage,
    scale: f32,
    cursor: Option<Point<Pixels>>,
    cx: &mut Context<'_, Self>,
  ) -> Self {
    Self {
      output,
      picture: image.to_gpui(),
      image,
      scale,
      cursor,
      drag_from: None,
      pins: Vec::new(),
      scrolled: 0.,
      focus: cx.focus_handle(),
      toolbar: RulerToolbar::new(),
    }
  }

  /// The device pixel under `p`
  fn pixel(&self, p: Point<Pixels>) -> (u32, u32) {
    let clamp = |v: Pixels, max: u32| ((v.as_f32() * self.scale).max(0.) as u32).min(max - 1);
    (
      clamp(p.x, self.image.width()),
      clamp(p.y, self.image.height()),
    )
  }

  /// The device pixel corner nearest to `p`
  fn corner(&self, p: Point<Pixels>) -> (u32, u32) {
    let clamp = |v: Pixels, max: u32| ((v.as_f32() * self.scale).round().max(0.) as u32).min(max);
    (
      clamp(p.x, self.image.width()),
      clamp(p.y, self.image.height()),
    )
  }

  fn hovered(&self, cx: &Context<Self>) -> bool {
    cx.try_global::<RulerState>()
      .is_some_and(|s| s.hovered_monitor == self.output)
  }

  /// What the cursor measures now
  fn live(&self, cx: &Context<Self>) -> Option<Measurement> {
    let state = cx.try_global::<RulerState>()?;
    if state.hovered_monitor != self.output {
      return None;
    }
    let cursor = self.cursor?;
    match state.mode {
      RulerMode::Bounds => Measurement::bounds(self.drag_from?, self.corner(cursor)),
      mode => Some(Measurement::spacing(
        &self.image,
        self.pixel(cursor),
        mode.axes(),
        state.tolerance,
      )),
    }
  }

  fn on_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
    if self.cursor == Some(e.position) {
      return;
    }
    self.cursor = Some(e.position);
    if let Some(state) = RulerState::get(cx)
      && state.hovered_monitor != self.output
    {
      state.hovered_monitor = self.output.clone();
      RulerState::refresh_all(cx);
    }
    cx.notify();
  }

  fn on_press(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
    let Some(mode) = cx.try_global::<RulerState>().map(|s| s.mode) else {
      return;
    };
    self.cursor = Some(e.position);
    if mode == RulerMode::Bounds {
      self.drag_from = Some(self.corner(e.position));
    } else if let Some(live) = self.live(cx) {
      self.pins.push(live);
    }
    cx.notify();
  }

  fn on_release(&mut self, e: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
    if self.drag_from.is_none() {
      return;
    }
    self.cursor = Some(e.position);
    self.pins.extend(self.live(cx));
    self.drag_from = None;
    cx.notify();
  }

  /// Removes the newest pin under `p`
  fn unpin_at(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
    let device = (p.x.as_f32() * self.scale, p.y.as_f32() * self.scale);
    if let Some(i) = self
      .pins
      .iter()
      .rposition(|m| m.hit(device, HIT_SLACK * self.scale))
    {
      self.pins.remove(i);
      cx.notify();
    }
  }

  fn on_scroll(&mut self, e: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
    let line = window.line_height().as_f32();
    // scrolling up raises the tolerance
    self.scrolled -= e.delta.pixel_delta(window.line_height()).y.as_f32();
    let steps = (self.scrolled / line).trunc();
    if steps != 0. {
      self.scrolled -= steps * line;
      RulerState::adjust_tolerance(steps as i32, cx);
    }
  }

  fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    let key = e.keystroke.key.as_str();
    match key {
      "escape" => RulerState::close(Some(window), cx),
      "backspace" => {
        self.pins.pop();
        cx.notify();
      }
      "delete" => {
        self.pins.clear();
        cx.notify();
      }
      _ => {
        if let Some(mode) = RulerMode::from_key(key) {
          self.drag_from = None;
          RulerState::set_mode(mode, cx);
        }
      }
    }
  }

  fn measurement(&self, m: &Measurement, accent: Hsla, cx: &Context<Self>) -> AnyElement {
    let at = |v: u32| px(v as f32 / self.scale);
    let from = |s: Span| at(s.start);
    let len = |s: Span| at(s.end + 1) - at(s.start);
    let one = px(1.);
    let cap = px(CAP);
    let rect = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
      div().absolute().left(x).top(y).w(w).h(h).bg(accent)
    };

    let mut root = div().absolute().inset_0();
    let label_at = if let (true, Some(h), Some(v)) = (m.boxed, m.h, m.v) {
      root = root.child(
        div()
          .absolute()
          .left(from(h))
          .top(from(v))
          .w(len(h))
          .h(len(v))
          .border_1()
          .border_color(accent)
          .bg(Hsla {
            a: BOX_FILL_ALPHA,
            ..accent
          }),
      );
      (from(h) + len(h), from(v) + len(v))
    } else {
      let (x, y) = (at(m.at.0), at(m.at.1));
      if let Some(h) = m.h {
        root = root
          .child(rect(from(h), y, len(h), one))
          .child(rect(from(h), y - cap / 2., one, cap))
          .child(rect(from(h) + len(h) - one, y - cap / 2., one, cap));
      }
      if let Some(v) = m.v {
        root = root
          .child(rect(x, from(v), one, len(v)))
          .child(rect(x - cap / 2., from(v), cap, one))
          .child(rect(x - cap / 2., from(v) + len(v) - one, cap, one));
      }
      (x, y)
    };

    root
      .child(
        div()
          .absolute()
          .left(label_at.0 + px(LABEL_OFFSET))
          .top(label_at.1 + px(LABEL_OFFSET))
          .child(
            Tag::secondary()
              .rounded_full()
              .font_family(cx.theme().mono_font_family.clone())
              .child(m.label()),
          ),
      )
      .into_any_element()
  }
}

impl Render for Overlay {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(state) = cx.try_global::<RulerState>() else {
      return div().size_full();
    };
    let (mode, tolerance) = (state.mode, state.tolerance);
    let accent = cx.theme().primary;
    let live = self.live(cx);
    let measurements: Vec<AnyElement> = (self.pins.iter().chain(live.as_ref()))
      .map(|m| self.measurement(m, accent, cx))
      .collect();
    let toolbar = if self.hovered(cx) {
      let duration = animation_duration(SLIDE_ANIMATION, cx);
      Some(
        self
          .toolbar
          .render(mode, tolerance, duration, window, cx)
          .into_any_element(),
      )
    } else {
      None
    };

    div()
      .track_focus(&self.focus)
      .relative()
      .size_full()
      .cursor(CursorStyle::Crosshair)
      .on_mouse_move(cx.listener(Self::on_move))
      .on_key_down(cx.listener(Self::on_key))
      .on_scroll(cx.listener(Self::on_scroll))
      .on_mouse_down(MouseButton::Left, cx.listener(Self::on_press))
      .on_mouse_up(MouseButton::Left, cx.listener(Self::on_release))
      .on_mouse_down(
        MouseButton::Right,
        cx.listener(|this, e: &MouseDownEvent, _, cx| this.unpin_at(e.position, cx)),
      )
      .child(img(self.picture.clone()).size_full())
      .children(measurements)
      .children(toolbar)
  }
}
