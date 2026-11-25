use std::{cell::RefCell, collections::HashMap, sync::Arc};

use anyhow::{Context as _, Result};
use corona_capture::{
  image::{ImageBuffer, Rgba, imageops},
  shm_dmabuf,
};
use corona_components::assets::image_source;
use corona_config::{Config, ConfigProvider};
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::{App, Asset, Dmabuf, Global, ImageAssetLoader, ImageSource, RenderImage, Task};
use tracing::error;

mod client;

/// Shows the configured wallpaper on every display, again when it changes
///
/// It is plain `wl_shm` surfaces on a Wayland connection of its own: drawn by gpui, two
/// fullscreen windows cost the GPU about 100 MiB each.
pub fn init(cx: &mut App) {
  reload(cx);
  cx.ipc_mut().register::<Reload>();
  let mut last = cx.config().wallpaper.clone();
  cx.observe_global::<Config>(move |cx| {
    let now = cx.config().wallpaper.clone();
    if now == last {
      return;
    }
    last = now;
    reload(cx);
  })
  .detach();
}

/// Keeps the surfaces of the shown wallpaper up
#[derive(Default)]
struct Shown {
  task: Option<Task<()>>,
  /// The loaded image, in RAM
  image: Option<Arc<RenderImage>>,
  /// By pixel size and corner radius
  thumbnails: RefCell<HashMap<(u32, u32, u32), Arc<Dmabuf>>>,
}

impl Global for Shown {}

/// Reads the configured image again and shows it, as after the file changed
pub fn reload(cx: &mut App) {
  // the old surfaces go first, so both are never up at once
  drop(cx.default_global::<Shown>().task.take());
  *cx.global_mut::<Shown>() = Shown::default();
  let task = start(cx);
  cx.global_mut::<Shown>().task = task;
}

/// Shows the wallpaper again from its file
pub struct Reload;

impl IpcCommand for Reload {
  const COMMAND: &'static str = "wallpaper:reload";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    reload(cx);
    Ok(())
  }
}

pub fn configured(cx: &App) -> Option<ImageSource> {
  cx.config().wallpaper.path.as_deref().map(image_source)
}

/// Loads the image, then keeps the surfaces up for as long as the task lives
fn start(cx: &mut App) -> Option<Task<()>> {
  let ImageSource::Resource(resource) = configured(cx)? else {
    return None;
  };
  let load = ImageAssetLoader::load(resource, cx);
  Some(cx.spawn(async move |cx| {
    let image = match load.await {
      Ok(image) => image,
      Err(e) => return error!("failed to load wallpaper: {e:#}"),
    };
    let shown = image.clone();
    cx.update(|cx| cx.global_mut::<Shown>().image = Some(shown));
    let _surfaces = match client::spawn(image) {
      Ok(surfaces) => surfaces,
      Err(e) => return error!("failed to show wallpaper: {e:#}"),
    };
    std::future::pending::<()>().await;
  }))
}

/// The wallpaper cropped and scaled to `size` pixels with `radius` pixel round
/// corners, for a [`corona_capture::FrameView`] drawing it from RAM; none
/// before it loaded
pub fn thumbnail(cx: &App, size: (u32, u32), radius: u32) -> Option<Arc<Dmabuf>> {
  let shown = cx.try_global::<Shown>()?;
  let image = shown.image.as_ref()?;
  let key = (size.0, size.1, radius);
  if let Some(cached) = shown.thumbnails.borrow().get(&key) {
    return Some(cached.clone());
  }
  let made = thumbnailed(image, size, radius)
    .inspect_err(|e| error!("failed to scale wallpaper: {e:#}"))
    .ok()?;
  let mut thumbnails = shown.thumbnails.borrow_mut();
  // card widths follow the windows, so sizes pile up
  if thumbnails.len() >= 32 {
    thumbnails.clear();
  }
  thumbnails.insert(key, made.clone());
  Some(made)
}

fn thumbnailed(image: &RenderImage, size: (u32, u32), radius: u32) -> Result<Arc<Dmabuf>> {
  let pixels = pixels(image)?;
  let ((x, y, w, h), _) = cover(pixels.dimensions(), size);
  let view = imageops::crop_imm(&pixels, x, y, w, h);
  let mut out = imageops::thumbnail(&*view, size.0.max(1), size.1.max(1));
  round_corners(&mut out, radius);
  shm_dmabuf(out.width(), out.height(), out.as_raw(), false)
}

/// Fades the corners outside a `radius` circle out, premultiplied as wl_shm wants
fn round_corners(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, radius: u32) {
  let (w, h) = image.dimensions();
  let r = radius.min(w / 2).min(h / 2) as f32;
  if r <= 0. {
    return;
  }
  for (x, y, px) in image.enumerate_pixels_mut() {
    // distance into the corner square, from the circle's center
    let dx = (r - (x as f32 + 0.5)).max((x as f32 + 0.5) - (w as f32 - r));
    let dy = (r - (y as f32 + 0.5)).max((y as f32 + 0.5) - (h as f32 - r));
    if dx <= 0. || dy <= 0. {
      continue;
    }
    let coverage = (r - dx.hypot(dy) + 0.5).clamp(0., 1.);
    for c in &mut px.0 {
      *c = (f32::from(*c) * coverage).round() as u8;
    }
  }
}

/// The image's BGRA pixels; scaling treats all four channels alike
fn pixels(image: &RenderImage) -> Result<ImageBuffer<Rgba<u8>, &[u8]>> {
  let size = image.size(0);
  let (width, height) = (size.width.0 as u32, size.height.0 as u32);
  ImageBuffer::from_raw(width, height, image.as_bytes(0).context("no frame")?)
    .context("pixels do not fit the size")
}

/// `image` cropped to the aspect of `display` and scaled to fit it, never up
fn scaled(image: &RenderImage, display: (u32, u32)) -> Result<Arc<Dmabuf>> {
  let pixels = pixels(image)?;
  let ((x, y, w, h), (out_w, out_h)) = cover(pixels.dimensions(), display);
  let view = imageops::crop_imm(&pixels, x, y, w, h);
  // a plain copy when the sizes match
  let out = imageops::resize(&*view, out_w, out_h, imageops::FilterType::Lanczos3);
  shm_dmabuf(out_w, out_h, out.as_raw(), true)
}

/// The centered crop `(x, y, w, h)` of a `src` image with the aspect of `display`,
/// and the size to scale it to: `display`, or the crop itself when that is smaller
fn cover(src: (u32, u32), display: (u32, u32)) -> ((u32, u32, u32, u32), (u32, u32)) {
  let (sw, sh) = (u64::from(src.0), u64::from(src.1));
  let (dw, dh) = (u64::from(display.0), u64::from(display.1));
  let (w, h) = if sw * dh > dw * sh {
    ((sh * dw / dh).max(1), sh)
  } else {
    (sw, (sw * dh / dw).max(1))
  };
  let out = if w <= dw { (w, h) } else { (dw, dh) };
  (
    (
      ((sw - w) / 2) as u32,
      ((sh - h) / 2) as u32,
      w as u32,
      h as u32,
    ),
    (out.0 as u32, out.1 as u32),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn corners_fade_out() {
    let mut image = ImageBuffer::from_pixel(20, 10, Rgba([200u8, 100, 50, 255]));
    round_corners(&mut image, 4);
    // the very corner is gone, the middle and the edge centers stay
    assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
    assert_eq!(image.get_pixel(19, 9).0, [0; 4]);
    assert_eq!(image.get_pixel(10, 5).0, [200, 100, 50, 255]);
    assert_eq!(image.get_pixel(10, 0).0, [200, 100, 50, 255]);
    assert_eq!(image.get_pixel(0, 5).0, [200, 100, 50, 255]);
    // premultiplied: no channel above alpha
    let edge = image.get_pixel(1, 1).0;
    assert!(edge[3] < 255 && edge[0] <= edge[3]);
  }

  #[test]
  fn crops_to_the_display_aspect() {
    // wider than the display: crop the sides, scale down
    assert_eq!(
      cover((4000, 1000), (1000, 1000)),
      ((1500, 0, 1000, 1000), (1000, 1000))
    );
    // taller than the display: crop top and bottom
    assert_eq!(
      cover((1000, 3000), (500, 500)),
      ((0, 1000, 1000, 1000), (500, 500))
    );
    // smaller than the display: no upscaling
    assert_eq!(
      cover((400, 300), (1920, 1080)),
      ((0, 37, 400, 225), (400, 225))
    );
  }
}
