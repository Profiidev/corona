use std::sync::Arc;

use corona_capture::{
  RgbaImageExt,
  image::{Rgba, RgbaImage},
};
use gpui_kit::{
  Context, CursorStyle, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
  MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Render, RenderImage, Styled,
  Window,
  component::{ActiveTheme, tag::Tag},
  div, img,
  prelude::FluentBuilder,
  px, rgb,
};

use crate::overlays::{OverlayState, colorpicker::state::ColorPickerState};

const DIAMETER: f32 = 180.;
const GRID: u32 = 11;
const BORDER: f32 = 4.;

struct Zoom {
  pixel: (u32, u32),
  color: Rgba<u8>,
  picture: Arc<RenderImage>,
}

pub struct Overlay {
  output: String,
  image: RgbaImage,
  scale: f32,
  picture: Arc<RenderImage>,
  cursor: Option<Point<Pixels>>,
  zoom: Option<Zoom>,
  stale: Vec<Arc<RenderImage>>,
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
      zoom: None,
      stale: Vec::new(),
      focus: cx.focus_handle(),
    }
  }

  fn update_zoom(&mut self) {
    let Some(cursor) = self.cursor else {
      return;
    };
    let ratio = self.scale;
    let clamp = |v: Pixels, max: u32| ((v.as_f32() * ratio).max(0.) as u32).min(max - 1);
    let pixel = (
      clamp(cursor.x, self.image.width()),
      clamp(cursor.y, self.image.height()),
    );
    if self.zoom.as_ref().is_some_and(|z| z.pixel == pixel) {
      return;
    }

    let size = (DIAMETER * ratio).round() as u32;
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

  fn magnifier(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
    let hovered = cx.try_global::<ColorPickerState>()?.hovered_monitor == self.output;
    if !hovered {
      return None;
    }
    let (cursor, zoom) = (self.cursor?, self.zoom.as_ref()?);
    let Rgba([r, g, b, _]) = zoom.color;
    let color = rgb(u32::from_be_bytes([0, r, g, b]));
    let size = px(DIAMETER);

    Some(
      div()
        .absolute()
        .left(cursor.x - size / 2.)
        .top(cursor.y - size / 2.)
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
        ),
    )
  }
}

impl Render for Overlay {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    for picture in self.stale.drain(..) {
      let _ = window.drop_image(picture);
    }
    self.update_zoom();

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
      .child(img(self.picture.clone()).size_full())
      .when_some(self.magnifier(cx), |d, m| d.child(m))
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
}
