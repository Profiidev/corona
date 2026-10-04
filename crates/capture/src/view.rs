use std::sync::Arc;

use futures::StreamExt;
use gpui_kit::{
  App, Context, Dmabuf, IntoElement, ParentElement, Render, RenderOnce, Size, StyleRefinement,
  Styled, Task, Window, canvas, div, prelude::FluentBuilder,
};

use crate::capture_window;

/// Draws a captured frame straight from its GPU memory, stretched to the
/// element's bounds. Size it like any element, e.g. `.size_full()`.
#[derive(IntoElement)]
pub struct FrameView {
  surface: Arc<Dmabuf>,
  style: StyleRefinement,
}

impl FrameView {
  pub fn new(surface: Arc<Dmabuf>) -> Self {
    Self {
      surface,
      style: StyleRefinement::default(),
    }
  }
}

impl Styled for FrameView {
  fn style(&mut self) -> &mut StyleRefinement {
    &mut self.style
  }
}

impl RenderOnce for FrameView {
  fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
    let surface = self.surface;
    let mut canvas = canvas(
      |_, _, _| (),
      move |bounds, _, window, _| window.paint_dmabuf(bounds, surface),
    );
    *canvas.style() = self.style;
    canvas
  }
}

/// Shows a window live, at most `max_fps` frames a second. Create it with
/// `cx.new(|cx| LiveCapture::window(&window.address, 10, cx))` and size it from
/// the parent; it notifies on every new frame.
pub struct LiveCapture {
  frame: Option<Arc<Dmabuf>>,
  // Dropping it drops the receiver, which stops the capture thread.
  _capture: Task<()>,
}

impl LiveCapture {
  /// `address` is the Hyprland window address, e.g. `0x55d2a4e1b2c0`.
  pub fn window(address: &str, max_fps: u32, cx: &mut Context<Self>) -> Self {
    let address = u64::from_str_radix(address.trim_start_matches("0x"), 16);
    let capture = cx.spawn(async move |this, cx| {
      let frames = address
        .map_err(anyhow::Error::from)
        .and_then(|address| capture_window(address, max_fps));
      let mut frames = match frames {
        Ok(frames) => frames,
        Err(e) => return tracing::warn!("live capture: {e:#}"),
      };
      while let Some(frame) = frames.next().await {
        let shown = this.update(cx, |this, cx| {
          this.frame = Some(frame);
          cx.notify();
        });
        if shown.is_err() {
          break;
        }
      }
    });
    Self {
      frame: None,
      _capture: capture,
    }
  }

  /// The latest frame's size in pixels, e.g. to keep the aspect ratio.
  pub fn frame_size(&self) -> Option<Size<u32>> {
    self.frame.as_ref().map(|f| Size::new(f.width, f.height))
  }
}

impl Render for LiveCapture {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div().size_full().when_some(self.frame.clone(), |d, frame| {
      d.child(FrameView::new(frame).size_full())
    })
  }
}
