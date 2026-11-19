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
  toolbar::RulerToolbar,
};
use crate::overlay::screenshot::overlay::SLIDE_ANIMATION;
use corona_surface::overlay::OverlayState;

/// How close (logical px) a right click must be to a pin to remove it
const HIT_SLACK: f32 = 6.;
/// Length of the caps at the ends of a line
const CAP: f32 = 10.;
const LABEL_OFFSET: f32 = 12.;
const BOX_FILL_ALPHA: f32 = 0.12;

/// The device pixel under logical `v` on an axis `len` device pixels long
fn device_pixel(v: Pixels, scale: f32, len: u32) -> u32 {
  ((v.as_f32() * scale).max(0.) as u32).min(len.saturating_sub(1))
}

/// The device pixel corner nearest to logical `v` on an axis `len` device pixels long
fn device_corner(v: Pixels, scale: f32, len: u32) -> u32 {
  ((v.as_f32() * scale).round().max(0.) as u32).min(len)
}

/// Whole lines scrolled so far, taken out of `scrolled`; scrolling up (negative `delta`)
/// counts up
fn scroll_steps(scrolled: &mut f32, delta: f32, line: f32) -> i32 {
  *scrolled -= delta;
  let steps = (*scrolled / line).trunc();
  *scrolled -= steps * line;
  steps as i32
}

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
    (
      device_pixel(p.x, self.scale, self.image.width()),
      device_pixel(p.y, self.scale, self.image.height()),
    )
  }

  /// The device pixel corner nearest to `p`
  fn corner(&self, p: Point<Pixels>) -> (u32, u32) {
    (
      device_corner(p.x, self.scale, self.image.width()),
      device_corner(p.y, self.scale, self.image.height()),
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
    let line = window.line_height();
    let delta = e.delta.pixel_delta(line).y.as_f32();
    let steps = scroll_steps(&mut self.scrolled, delta, line.as_f32());
    if steps != 0 {
      RulerState::adjust_tolerance(steps, cx);
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

#[cfg(test)]
mod tests {
  use corona_capture::image::Rgba;
  use corona_surface::testing::{FakeCompositor, setup};
  use gpui_kit::{self as gpui, Entity, Modifiers, TestAppContext, VisualTestContext, point};

  use super::*;

  const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
  const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

  #[test]
  fn logical_to_device_at_scale() {
    // 1.5: logical 10 is device 15
    assert_eq!(device_pixel(px(10.), 1.5, 100), 15);
    assert_eq!(device_pixel(px(10.3), 1.5, 100), 15);
    // past the edge is the last pixel, before it the first
    assert_eq!(device_pixel(px(80.), 1.5, 100), 99);
    assert_eq!(device_pixel(px(-3.), 1.5, 100), 0);
    // corners round and may sit on the far edge
    assert_eq!(device_corner(px(10.3), 1.5, 100), 15);
    assert_eq!(device_corner(px(10.4), 1.5, 100), 16);
    assert_eq!(device_corner(px(80.), 1.5, 100), 100);
    assert_eq!(device_corner(px(-3.), 1.5, 100), 0);
  }

  #[test]
  fn scrolling_steps_by_whole_lines() {
    let mut scrolled = 0.;
    // half a line up: nothing yet
    assert_eq!(scroll_steps(&mut scrolled, -10., 20.), 0);
    // the other half completes a step up
    assert_eq!(scroll_steps(&mut scrolled, -10., 20.), 1);
    assert_eq!(scrolled, 0.);
    assert_eq!(scroll_steps(&mut scrolled, 45., 20.), -2);
    assert_eq!(scrolled, -5.);
  }

  /// 200×100 device pixels at scale 2, white with a black column at x = 150 and a
  /// black row at y = 20
  fn open(mode: RulerMode, cx: &mut TestAppContext) -> (Entity<Overlay>, &mut VisualTestContext) {
    setup(FakeCompositor::default(), cx);
    cx.update(|cx| cx.set_global(RulerState::test(mode, 30, "DP-1")));
    let image = RgbaImage::from_fn(
      200,
      100,
      |x, y| if x == 150 || y == 20 { BLACK } else { WHITE },
    );
    let (view, vcx) = cx.add_window_view(|_, cx| Overlay::new("DP-1".into(), image, 2., None, cx));
    vcx.update(|window, cx| window.focus(&view.read(cx).focus.clone(), cx));
    (view, vcx)
  }

  fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
  }

  fn labels(view: &Entity<Overlay>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |o, _| o.pins.iter().map(Measurement::label).collect())
  }

  #[gpui::test]
  fn spacing_click_pins_and_right_click_unpins(cx: &mut TestAppContext) {
    let (view, cx) = open(RulerMode::Spacing, cx);
    let none = Modifiers::none();
    // device (50, 50): 150 px to the column, rows 21..=99 below the black row
    cx.simulate_click(at(25., 25.), none);
    assert_eq!(labels(&view, cx), ["150 × 79"]);

    // far from both lines
    cx.simulate_mouse_down(at(90., 45.), MouseButton::Right, none);
    assert_eq!(labels(&view, cx).len(), 1);
    // on the horizontal line
    cx.simulate_mouse_down(at(40., 25.2), MouseButton::Right, none);
    assert!(labels(&view, cx).is_empty());
  }

  #[gpui::test]
  fn bounds_pin_on_release_only_with_area(cx: &mut TestAppContext) {
    let (view, cx) = open(RulerMode::Bounds, cx);
    let none = Modifiers::none();
    cx.simulate_mouse_down(at(2., 2.), MouseButton::Left, none);
    assert!(labels(&view, cx).is_empty());
    cx.simulate_mouse_move(at(7., 4.5), MouseButton::Left, none);
    cx.simulate_mouse_up(at(7., 4.5), MouseButton::Left, none);
    assert_eq!(labels(&view, cx), ["10 × 5"]);

    // a click without dragging
    cx.simulate_click(at(9., 9.), none);
    assert_eq!(labels(&view, cx).len(), 1);
  }

  #[gpui::test]
  fn keys_remove_pins_and_switch_modes(cx: &mut TestAppContext) {
    let (view, cx) = open(RulerMode::Horizontal, cx);
    let none = Modifiers::none();
    for x in [2., 4., 6.] {
      cx.simulate_click(at(x, 8.), none);
    }
    assert_eq!(labels(&view, cx), ["150", "150", "150"]);
    cx.simulate_keystrokes("backspace");
    assert_eq!(labels(&view, cx).len(), 2);
    cx.simulate_keystrokes("delete");
    assert!(labels(&view, cx).is_empty());

    // switching mode drops a running drag, so its release pins nothing
    cx.simulate_keystrokes("1");
    cx.simulate_mouse_down(at(2., 2.), MouseButton::Left, none);
    cx.simulate_keystrokes("2");
    assert_eq!(
      cx.read(|cx| cx.global::<RulerState>().mode),
      RulerMode::Spacing
    );
    cx.simulate_mouse_up(at(7., 7.), MouseButton::Left, none);
    assert!(labels(&view, cx).is_empty());
  }
}
