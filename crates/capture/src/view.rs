use std::sync::Arc;

use futures::StreamExt;
use gpui_kit::{
  App, Context, Corners, Dmabuf, IntoElement, ParentElement, Pixels, Render, RenderOnce, Size,
  StyleRefinement, Styled, Task, Window, canvas, div, prelude::FluentBuilder,
};

use crate::capture_window;

/// Draws a captured frame straight from its GPU memory, stretched to the
/// element's bounds. Size it like any element, e.g. `.size_full()`, and round it
/// with `.rounded(..)`.
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
    let radii = &self.style.corner_radii;
    let corner_radii = Corners {
      top_left: radii.top_left.unwrap_or_default(),
      top_right: radii.top_right.unwrap_or_default(),
      bottom_right: radii.bottom_right.unwrap_or_default(),
      bottom_left: radii.bottom_left.unwrap_or_default(),
    };
    let mut canvas = canvas(
      |_, _, _| (),
      move |bounds, _, window, _| {
        let corner_radii = corner_radii.to_pixels(window.rem_size());
        window.paint_dmabuf(bounds, corner_radii, surface)
      },
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
  corner_radius: Pixels,
  // Dropping it drops the receiver, which stops the capture thread.
  _capture: Task<()>,
}

impl LiveCapture {
  /// `address` is the Hyprland window address, e.g. `0x55d2a4e1b2c0`.
  pub fn window(address: &str, max_fps: u32, cx: &mut Context<Self>) -> Self {
    let frames = parse_address(address).and_then(|address| capture_window(address, max_fps));
    Self::from_frames(frames, cx)
  }

  fn from_frames(
    frames: anyhow::Result<futures::channel::mpsc::Receiver<Arc<Dmabuf>>>,
    cx: &mut Context<Self>,
  ) -> Self {
    let capture = cx.spawn(async move |this, cx| {
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
      corner_radius: Pixels::ZERO,
      _capture: capture,
    }
  }

  pub fn rounded(mut self, radius: Pixels) -> Self {
    self.corner_radius = radius;
    self
  }

  /// The latest frame's size in pixels, e.g. to keep the aspect ratio.
  pub fn frame_size(&self) -> Option<Size<u32>> {
    self.frame.as_ref().map(|f| Size::new(f.width, f.height))
  }
}

/// A Hyprland window address, hex with or without `0x`.
fn parse_address(address: &str) -> anyhow::Result<u64> {
  Ok(u64::from_str_radix(address.trim_start_matches("0x"), 16)?)
}

impl Render for LiveCapture {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div().size_full().when_some(self.frame.clone(), |d, frame| {
      d.child(
        FrameView::new(frame)
          .size_full()
          .rounded(self.corner_radius),
      )
    })
  }
}

#[cfg(test)]
mod tests {
  use std::os::fd::OwnedFd;

  use futures::{SinkExt, channel::mpsc::channel, executor::block_on};
  use gpui_kit::{self as gpui, AppContext, DmabufPlane, TestAppContext};
  use rustix::fs::{MemfdFlags, memfd_create};

  use super::*;

  #[test]
  fn addresses() {
    assert_eq!(parse_address("0x55d2a4e1b2c0").unwrap(), 0x55d2_a4e1_b2c0);
    assert_eq!(parse_address("55D2a4e1b2c0").unwrap(), 0x55d2_a4e1_b2c0);
    assert_eq!(parse_address("0x0").unwrap(), 0);
    assert_eq!(parse_address("ffffffffffffffff").unwrap(), u64::MAX);
    for bad in ["", "0x", "0xg", "1ffffffffffffffff", " 0x1", "-1", "0X1f"] {
      assert!(parse_address(bad).is_err(), "{bad}");
    }
  }

  fn surface(width: u32, height: u32) -> Arc<Dmabuf> {
    let fd: OwnedFd = memfd_create("corona-test", MemfdFlags::CLOEXEC).unwrap();
    Arc::new(Dmabuf {
      width,
      height,
      planes: vec![DmabufPlane {
        fd,
        offset: 0,
        stride: width * 4,
      }],
      modifier: None,
      opaque: true,
    })
  }

  #[gpui::test]
  fn shows_the_latest_frame(cx: &mut TestAppContext) {
    let (mut tx, rx) = channel(0);
    let live = cx.new(|cx| LiveCapture::from_frames(Ok(rx), cx).rounded(Pixels::from(4.)));
    cx.run_until_parked();
    live.read_with(cx, |live, _| {
      assert_eq!(live.frame_size(), None);
      assert_eq!(live.corner_radius, Pixels::from(4.));
    });

    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&live, move |_, _| count.set(count.get() + 1))
        .detach()
    });
    // the sender's own slot: one frame in flight at a time
    tx.try_send(surface(640, 480)).unwrap();
    cx.run_until_parked();
    tx.try_send(surface(320, 200)).unwrap();
    cx.run_until_parked();
    live.read_with(cx, |live, _| {
      assert_eq!(live.frame_size(), Some(Size::new(320, 200)))
    });
    assert_eq!(notified.get(), 2);

    // dropping the view stops taking frames
    drop(live);
    cx.update(|_| {});
    cx.run_until_parked();
    assert!(tx.is_closed());
  }

  #[gpui::test]
  fn failed_captures_show_nothing(cx: &mut TestAppContext) {
    let live = cx.new(|cx| LiveCapture::from_frames(Err(anyhow::anyhow!("no compositor")), cx));
    cx.run_until_parked();
    live.read_with(cx, |live, _| assert_eq!(live.frame_size(), None));
    // a bad address never reaches Wayland
    let bad = cx.new(|cx| LiveCapture::window("zz", 10, cx));
    cx.run_until_parked();
    bad.read_with(cx, |live, _| assert_eq!(live.frame_size(), None));
  }

  #[gpui::test]
  fn ended_captures_keep_the_last_frame(cx: &mut TestAppContext) {
    let (mut tx, rx) = channel(1);
    block_on(tx.send(surface(8, 8))).unwrap();
    drop(tx);
    let live = cx.new(|cx| LiveCapture::from_frames(Ok(rx), cx));
    cx.run_until_parked();
    live.read_with(cx, |live, _| {
      assert_eq!(live.frame_size(), Some(Size::new(8, 8)))
    });
  }
}
