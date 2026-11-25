use std::{mem, sync::Arc};

use corona_capture::{
  RgbaImageExt,
  image::{Rgba, RgbaImage},
};
use gpui_kit::{
  App, Bounds, Context, CursorStyle, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
  MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Render, RenderImage,
  Size, Styled, Window,
  base::h_flex,
  component::{ActiveTheme, ThemeStyled, tag::Tag},
  div, img, point,
  prelude::FluentBuilder,
  px, rgb,
};

use crate::overlay::{
  colorpicker::state::ColorPickerState,
  live::{LiveFeed, LiveView, Rect, Tracked, tracker},
  screenshot::toolbar::live_button,
};
use corona_surface::overlay::OverlayState;

const DIAMETER: f32 = 180.;
const GRID: u32 = 11;
const BORDER: f32 = 4.;
/// How far a magnifier beside the cursor sits from it
const BESIDE: f32 = 24.;

struct Zoom {
  pixel: (u32, u32),
  color: Rgba<u8>,
  picture: Arc<RenderImage>,
}

pub struct Overlay {
  output: String,
  /// The frozen capture, or the clean image of the live screen
  image: RgbaImage,
  scale: f32,
  /// The frozen capture shown, none while live
  picture: Option<Arc<RenderImage>>,
  cursor: Option<Point<Pixels>>,
  zoom: Option<Zoom>,
  stale: Vec<Arc<RenderImage>>,
  fps: u32,
  feed: LiveFeed,
  painted: Tracked,
  pub focus: FocusHandle,
}

fn hex(Rgba([r, g, b, _]): Rgba<u8>) -> String {
  format!("#{r:02X}{g:02X}{b:02X}")
}

fn magnify(image: &RgbaImage, (cx, cy): (u32, u32), size: u32) -> RgbaImage {
  let half = (GRID / 2) as i64;
  RgbaImage::from_fn(size, size, |x, y| {
    let sx = cx as i64 - half + (x * GRID / size) as i64;
    let sy = cy as i64 - half + (y * GRID / size) as i64;
    match (u32::try_from(sx), u32::try_from(sy)) {
      (Ok(sx), Ok(sy)) if sx < image.width() && sy < image.height() => *image.get_pixel(sx, sy),
      _ => Rgba([0, 0, 0, 255]),
    }
  })
}

/// The magnifier's top left beside the cursor, away from the edges of `area`
fn magnifier_at(cursor: Point<Pixels>, size: Pixels, area: Size<Pixels>) -> Point<Pixels> {
  let gap = px(BESIDE);
  let place = |c: Pixels, max: Pixels| match c + gap + size <= max {
    true => c + gap,
    false => c - gap - size,
  };
  point(place(cursor.x, area.width), place(cursor.y, area.height))
}

impl Overlay {
  pub fn new(
    output: String,
    image: RgbaImage,
    scale: f32,
    cursor: Option<Point<Pixels>>,
    fps: u32,
    cx: &mut Context<'_, Self>,
  ) -> Self {
    Self {
      feed: LiveFeed::new(output.clone()),
      output,
      picture: (image.width() > 0).then(|| image.to_gpui()),
      image,
      scale,
      cursor,
      zoom: None,
      stale: Vec::new(),
      fps,
      painted: Tracked::default(),
      focus: cx.focus_handle(),
    }
  }

  /// The device pixel under the cursor
  fn cursor_pixel(&self) -> Option<(u32, u32)> {
    let cursor = self.cursor?;
    if self.image.width() == 0 || self.image.height() == 0 {
      return None;
    }
    let clamp = |v: Pixels, max: u32| ((v.as_f32() * self.scale).max(0.) as u32).min(max - 1);
    Some((
      clamp(cursor.x, self.image.width()),
      clamp(cursor.y, self.image.height()),
    ))
  }

  fn update_zoom(&mut self) {
    let Some(pixel) = self.cursor_pixel() else {
      return;
    };
    if self.zoom.as_ref().is_some_and(|z| z.pixel == pixel) {
      return;
    }

    let size = (DIAMETER * self.scale).round() as u32;
    let zoom = Zoom {
      pixel,
      color: *self.image.get_pixel(pixel.0, pixel.1),
      picture: magnify(&self.image, pixel, size).to_gpui(),
    };
    if let Some(old) = self.zoom.replace(zoom) {
      self.stale.push(old.picture);
    }
  }

  fn on_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
    if self.cursor == Some(e.position) {
      return;
    }
    self.cursor = Some(e.position);
    if let Some(state) = ColorPickerState::get(cx)
      && state.hovered_monitor != self.output
    {
      state.hovered_monitor = self.output.clone();
      ColorPickerState::refresh_all(cx);
    }
    cx.notify();
  }

  fn pick(&self, window: &mut Window, cx: &mut Context<Self>) {
    match &self.zoom {
      Some(zoom) => ColorPickerState::pick(hex(zoom.color), window, cx),
      None => ColorPickerState::close(Some(window), cx),
    }
  }

  /// Streams the live screen, or freezes on a capture taken without the drawings
  pub fn set_live(&mut self, live: bool, cx: &mut Context<Self>) {
    if live == self.feed.live() {
      return;
    }
    if live {
      self.stale.extend(self.picture.take());
      LiveFeed::start(self, self.fps, true, cx);
      cx.notify();
      return;
    }
    cx.spawn(async move |this, cx| {
      let frozen = LiveFeed::freeze(this.clone(), cx).await;
      let _ = this.update(cx, |v, cx| {
        match frozen {
          Ok(()) => v.picture = Some(v.image.to_gpui()),
          Err(e) => tracing::warn!("color picker freeze failed: {e:#}"),
        }
        cx.notify();
      });
    })
    .detach();
  }

  fn magnifier(&self, area: Size<Pixels>, cx: &Context<Self>) -> Option<impl IntoElement> {
    let state = cx.try_global::<ColorPickerState>()?;
    if state.hovered_monitor != self.output {
      return None;
    }
    // a centered magnifier would cover the sampled pixels; frozen looks the same as live
    let (cursor, zoom) = (self.cursor?, self.zoom.as_ref()?);
    let Rgba([r, g, b, _]) = zoom.color;
    let color = rgb(u32::from_be_bytes([0, r, g, b]));
    let size = px(DIAMETER);
    let at = magnifier_at(cursor, size, area);

    Some(
      div()
        .absolute()
        .left(at.x)
        .top(at.y)
        .size(size)
        .child(img(zoom.picture.clone()).size_full().rounded_full())
        .child(
          div()
            .absolute()
            .inset_0()
            .rounded_full()
            .border(px(BORDER))
            .border_color(color),
        )
        .child(
          div()
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .items_end()
            .pb(size * 0.2)
            .child(
              Tag::secondary()
                .text_sm()
                .font_family(cx.theme().mono_font_family.clone())
                .child(hex(zoom.color)),
            ),
        )
        .child(tracker(&self.painted)),
    )
  }

  /// The live switch at the bottom
  fn toolbar(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
    let state = cx.try_global::<ColorPickerState>()?;
    (state.hovered_monitor == self.output).then(|| {
      h_flex()
        .absolute()
        .bottom(px(24.))
        .left_0()
        .right_0()
        .justify_center()
        .child(
          h_flex()
            .relative()
            .popover_style(cx)
            .rounded_full()
            .p_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(live_button(
              "colorpicker-live",
              self.feed.live(),
              ColorPickerState::toggle_live,
            ))
            .child(tracker(&self.painted)),
        )
    })
  }
}

impl LiveView for Overlay {
  fn feed(&mut self) -> &mut LiveFeed {
    &mut self.feed
  }

  fn image(&mut self) -> &mut RgbaImage {
    &mut self.image
  }

  fn wanted(&self, cx: &App) -> Vec<Rect> {
    let hovered = cx
      .try_global::<ColorPickerState>()
      .is_some_and(|s| s.hovered_monitor == self.output);
    match self.cursor_pixel() {
      Some((x, y)) if hovered => {
        let half = GRID / 2;
        vec![Rect::new(
          x.saturating_sub(half),
          y.saturating_sub(half),
          GRID,
          GRID,
        )]
      }
      _ => Vec::new(),
    }
  }

  fn sampled(&mut self, cx: &mut Context<Self>) {
    if let Some(old) = self.zoom.take() {
      self.stale.push(old.picture);
    }
    cx.notify();
  }
}

impl Render for Overlay {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    for picture in self.stale.drain(..) {
      let _ = window.drop_image(picture);
    }
    let painted: Vec<Bounds<Pixels>> = mem::take(&mut *self.painted.borrow_mut());
    let live = self.feed.live();
    if live {
      let scale = self.scale;
      self
        .feed
        .drawn(painted.iter().map(|b| Rect::covering(*b, scale)).collect());
    }
    let shown = !live || (self.feed.ready && !self.feed.hidden);
    if shown {
      self.update_zoom();
    }
    let area = window.viewport_size();

    div()
      .track_focus(&self.focus)
      .relative()
      .size_full()
      .cursor(CursorStyle::Crosshair)
      .on_mouse_move(cx.listener(Self::on_move))
      .on_key_down(cx.listener(
        |this, e: &KeyDownEvent, window, cx| match e.keystroke.key.as_str() {
          "escape" => ColorPickerState::close(Some(window), cx),
          "enter" => this.pick(window, cx),
          "f" => ColorPickerState::toggle_live(cx),
          _ => {}
        },
      ))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, _: &MouseDownEvent, window, cx| this.pick(window, cx)),
      )
      .on_mouse_down(
        MouseButton::Right,
        cx.listener(|_, _: &MouseDownEvent, window, cx| ColorPickerState::close(Some(window), cx)),
      )
      .when_some(self.picture.clone(), |d, p| d.child(img(p).size_full()))
      .when(shown, |d| {
        d.when_some(self.magnifier(area, cx), |d, m| d.child(m))
          .when_some(self.toolbar(cx), |d, t| d.child(t))
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn magnify_centers_and_pads() {
    let mut image = RgbaImage::new(3, 3);
    image.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    // 11 px grid at 1:1, cursor on the red corner pixel.
    let zoom = magnify(&image, (0, 0), GRID);
    assert_eq!(*zoom.get_pixel(5, 5), Rgba([255, 0, 0, 255]));
    assert_eq!(*zoom.get_pixel(4, 5), Rgba([0, 0, 0, 255]));
    assert_eq!(hex(Rgba([0xF2, 0xA7, 0x5D, 255])), "#F2A75D");
  }

  #[test]
  fn magnifier_beside_flips_at_edges() {
    let area = Size::new(px(1000.), px(500.));
    let size = px(DIAMETER);
    let beside = magnifier_at(point(px(100.), px(100.)), size, area);
    assert_eq!(beside, point(px(124.), px(124.)));
    // too close to the bottom right: up and left of the cursor
    let flipped = magnifier_at(point(px(900.), px(400.)), size, area);
    assert_eq!(flipped, point(px(696.), px(196.)));
  }
}
