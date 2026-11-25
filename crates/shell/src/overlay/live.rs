//! Live mode of the capture overlays: the screen keeps moving under a transparent overlay,
//! which streams its monitor and reads only the pixels it needs into a "clean" image.
//!
//! The overlay is part of its own capture: pixels under its own drawings keep their last
//! clean value.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use anyhow::{Context as _, Result};
use corona_capture::{RgbaImageExt, capture_all, capture_output, image::RgbaImage};
use futures::StreamExt;
use gpui_kit::{
  App, AppContext, Bounds, Context, Dmabuf, IntoElement, Pixels, Styled, Task, WeakEntity, canvas,
};

use crate::overlay::screenshot::state::FREEZE_SETTLE;

/// `image` for [`corona_capture::FrameView`], drawn from RAM; none when empty
pub(crate) fn picture(image: &RgbaImage) -> Option<Arc<Dmabuf>> {
  if image.width() == 0 || image.height() == 0 {
    return None;
  }
  image
    .to_shm()
    .inspect_err(|e| tracing::warn!("frozen picture: {e:#}"))
    .ok()
}

/// How many renders' drawings stay masked, frames lag behind renders
const MASK_HISTORY: usize = 4;

/// A rect in device pixels
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
  pub x: u32,
  pub y: u32,
  pub w: u32,
  pub h: u32,
}

impl Rect {
  pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
    Self { x, y, w, h }
  }

  fn contains(&self, x: u32, y: u32) -> bool {
    (self.x..self.x.saturating_add(self.w)).contains(&x)
      && (self.y..self.y.saturating_add(self.h)).contains(&y)
  }

  /// The device pixels logical `b` touches at `scale`
  pub fn covering(b: Bounds<Pixels>, scale: f32) -> Self {
    let lo = |v: Pixels| (v.as_f32() * scale).floor().max(0.) as u32;
    let hi = |v: Pixels| (v.as_f32() * scale).ceil().max(0.) as u32;
    let (x, y) = (lo(b.left()), lo(b.top()));
    Self::new(
      x,
      y,
      hi(b.right()).saturating_sub(x),
      hi(b.bottom()).saturating_sub(y),
    )
  }
}

/// The capture rate: the monitor's refresh rate, at most `cap`
pub fn fps(refresh: f32, cap: Option<u32>) -> u32 {
  let refresh = (refresh.round() as u32).max(1);
  cap.map_or(refresh, |c| c.clamp(1, refresh))
}

/// Copies `patch` into `clean` at `at`, except the pixels inside `covered`
pub fn merge(clean: &mut RgbaImage, patch: &RgbaImage, at: (u32, u32), covered: &[Rect]) {
  for (px, py, pixel) in patch.enumerate_pixels() {
    let (x, y) = (at.0 + px, at.1 + py);
    if x < clean.width() && y < clean.height() && !covered.iter().any(|r| r.contains(x, y)) {
      clean.put_pixel(x, y, *pixel);
    }
  }
}

/// Bounds elements were painted at, filled by [`tracker`]
pub type Tracked = Rc<RefCell<Vec<Bounds<Pixels>>>>;

/// Records the bounds of its parent into `sink` each paint; the parent must be positioned
pub fn tracker(sink: &Tracked) -> impl IntoElement {
  let sink = sink.clone();
  canvas(
    move |bounds, _, _| sink.borrow_mut().push(bounds),
    |_, _, _, _| {},
  )
  .absolute()
  .size_full()
}

/// A view sampling its monitor live
pub trait LiveView: Sized + 'static {
  fn feed(&mut self) -> &mut LiveFeed;
  /// The clean image the view reads pixels from
  fn image(&mut self) -> &mut RgbaImage;
  /// The rects to read, empty when nothing is needed now
  fn wanted(&self, cx: &App) -> Vec<Rect>;
  /// The clean image changed
  fn sampled(&mut self, cx: &mut Context<Self>);
}

/// The live side of one overlay window. Dropping it stops the stream.
#[derive(Default)]
pub struct LiveFeed {
  output: String,
  stream: Option<Task<()>>,
  /// The clean image holds a whole frame
  pub ready: bool,
  /// The drawings are hidden for a capture
  pub hidden: bool,
  covered: Vec<Vec<Rect>>,
}

impl LiveFeed {
  pub fn new(output: String) -> Self {
    Self {
      output,
      ..Default::default()
    }
  }

  pub fn live(&self) -> bool {
    self.stream.is_some()
  }

  /// Streams the monitor of `v` at `fps`; `ready` says whether the clean image already
  /// holds a whole frame, a frozen one switching to live
  pub fn start<V: LiveView>(v: &mut V, fps: u32, ready: bool, cx: &mut Context<V>) {
    let feed = v.feed();
    feed.ready = ready;
    feed.hidden = false;
    let output = feed.output.clone();
    let task = cx.spawn(async move |this: WeakEntity<V>, cx| {
      let mut frames = match capture_output(output, fps) {
        Ok(frames) => frames,
        Err(e) => {
          tracing::error!("live capture failed: {e:#}");
          return;
        }
      };
      while let Some(frame) = frames.next().await {
        let Ok(Some((full, rects, covered))) = this.update(cx, |v, cx| {
          let full = !v.feed().ready;
          if !full && v.feed().hidden {
            return None;
          }
          let rects = if full {
            vec![Rect::new(0, 0, u32::MAX, u32::MAX)]
          } else {
            v.wanted(cx)
          };
          let covered = match full {
            true => Vec::new(),
            false => v.feed().covered.concat(),
          };
          (!rects.is_empty()).then_some((full, rects, covered))
        }) else {
          continue;
        };
        let patches = cx
          .background_spawn(async move { read(&frame, &rects) })
          .await;
        let applied = this.update(cx, |v, cx| {
          apply(v, full, patches, &covered);
          v.sampled(cx);
        });
        if applied.is_err() {
          return;
        }
      }
    });
    v.feed().stream = Some(task);
  }

  /// Stops the stream, the view stays frozen on its clean image
  pub fn stop(&mut self) {
    self.stream = None;
    self.hidden = false;
  }

  /// Remembers the device rects drawn this render, they stay masked a few frames
  pub fn drawn(&mut self, rects: Vec<Rect>) {
    self.covered.push(rects);
    if self.covered.len() > MASK_HISTORY {
      self.covered.remove(0);
    }
  }

  /// Hides the drawings and freezes on one capture of the whole monitor
  pub async fn freeze<V: LiveView>(this: WeakEntity<V>, cx: &mut gpui_kit::AsyncApp) -> Result<()> {
    let output = this.update(cx, |v, cx| {
      v.feed().hidden = true;
      cx.notify();
      v.feed().output.clone()
    })?;
    cx.background_executor().timer(FREEZE_SETTLE).await;
    let patches = cx
      .background_spawn(async move {
        let (_, frame) = capture_all([output])?
          .into_iter()
          .next()
          .context("no frame")?;
        read(&frame, &[Rect::new(0, 0, u32::MAX, u32::MAX)])
      })
      .await;
    this.update(cx, |v, cx| {
      let feed = v.feed();
      feed.stop();
      apply(v, true, patches, &[]);
      v.sampled(cx);
    })
  }
}

type Patches = Result<Vec<((u32, u32), RgbaImage)>>;

fn read(frame: &corona_capture::Frame, rects: &[Rect]) -> Patches {
  rects
    .iter()
    .map(|r| {
      let (x, y) = (r.x.min(frame.width()), r.y.min(frame.height()));
      Ok(((x, y), frame.read(x, y, r.w, r.h)?))
    })
    .collect()
}

fn apply<V: LiveView>(v: &mut V, full: bool, patches: Patches, covered: &[Rect]) {
  let patches = match patches {
    Ok(p) => p,
    Err(e) => {
      tracing::warn!("live readback failed: {e:#}");
      return;
    }
  };
  for (at, patch) in patches {
    if full {
      *v.image() = patch;
      v.feed().ready = true;
    } else {
      merge(v.image(), &patch, at, covered);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use corona_capture::image::Rgba;
  use gpui_kit::{point, px, size};

  const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);

  #[test]
  fn fps_caps_at_refresh() {
    assert_eq!(fps(143.9, None), 144);
    assert_eq!(fps(60., Some(30)), 30);
    assert_eq!(fps(60., Some(240)), 60);
    assert_eq!(fps(60., Some(0)), 1);
    assert_eq!(fps(0., None), 1);
  }

  #[test]
  fn merge_skips_covered_and_outside() {
    let mut clean = RgbaImage::new(4, 4);
    let patch = RgbaImage::from_pixel(3, 3, RED);
    merge(&mut clean, &patch, (2, 2), &[Rect::new(3, 3, 1, 1)]);
    assert_eq!(*clean.get_pixel(2, 2), RED);
    assert_eq!(*clean.get_pixel(3, 2), RED);
    assert_eq!(*clean.get_pixel(3, 3), Rgba([0, 0, 0, 0]));
    assert_eq!(*clean.get_pixel(1, 1), Rgba([0, 0, 0, 0]));
  }

  #[test]
  fn covering_rounds_outwards() {
    let b = Bounds::new(point(px(1.2), px(0.5)), size(px(2.), px(1.)));
    assert_eq!(Rect::covering(b, 2.), Rect::new(2, 1, 5, 2));
    assert_eq!(Rect::covering(b, 1.), Rect::new(1, 0, 3, 2));
  }
}
