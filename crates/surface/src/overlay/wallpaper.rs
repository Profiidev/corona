use std::sync::Arc;

use anyhow::{Context as _, Result};
use corona_capture::{
  image::{ImageBuffer, Rgba, imageops},
  shm_dmabuf,
};
use corona_components::assets::image_source;
use corona_config::{Config, ConfigProvider};
use gpui_kit::{App, Asset, Dmabuf, ImageAssetLoader, ImageSource, RenderImage, Task};
use tracing::error;

mod client;

/// Shows the configured wallpaper on every display, again when it changes
///
/// It is plain `wl_shm` surfaces on a Wayland connection of its own: drawn by gpui, two
/// fullscreen windows cost the GPU about 100 MiB each.
pub fn init(cx: &mut App) {
  let mut task = start(cx);
  let mut last = cx.config().wallpaper.clone();
  cx.observe_global::<Config>(move |cx| {
    let now = cx.config().wallpaper.clone();
    if now == last {
      return;
    }
    last = now;
    drop(task.take());
    task = start(cx);
  })
  .detach();
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
  Some(cx.spawn(async move |_| {
    let image = match load.await {
      Ok(image) => image,
      Err(e) => return error!("failed to load wallpaper: {e:#}"),
    };
    let _surfaces = match client::spawn(image) {
      Ok(surfaces) => surfaces,
      Err(e) => return error!("failed to show wallpaper: {e:#}"),
    };
    std::future::pending::<()>().await;
  }))
}

/// `image` cropped to the aspect of `display` and scaled to fit it, never up
fn scaled(image: &RenderImage, display: (u32, u32)) -> Result<Arc<Dmabuf>> {
  let size = image.size(0);
  let (width, height) = (size.width.0 as u32, size.height.0 as u32);
  // BGRA, but scaling treats all four channels alike
  let pixels =
    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, image.as_bytes(0).context("no frame")?)
      .context("pixels do not fit the size")?;
  let ((x, y, w, h), (out_w, out_h)) = cover((width, height), display);
  let view = imageops::crop_imm(&pixels, x, y, w, h);
  // a plain copy when the sizes match
  let out = imageops::resize(&*view, out_w, out_h, imageops::FilterType::Lanczos3);
  shm_dmabuf(out_w, out_h, out.as_raw())
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
