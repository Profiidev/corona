use std::{collections::HashMap, fs};

use anyhow::{Context, Result, bail};
use corona_capture::{
  Frame, RgbaImageExt,
  image::{
    RgbaImage,
    imageops::{self, FilterType},
  },
};
use gpui_kit::{
  App, AppContext, Bounds, ClipboardItem, Image as GpuiImage, ImageFormat, Pixels, Window,
};
use jiff::Zoned;
use tracing::{error, info};

use crate::overlays::{
  OverlayState,
  screenshot::state::{MonitorGeometry, ScreenshotState},
};

fn finish(png: Vec<u8>, cx: &mut App) {
  cx.write_to_clipboard(ClipboardItem::new_image(&GpuiImage::from_bytes(
    ImageFormat::Png,
    png.clone(),
  )));

  cx.background_spawn(async move {
    match save(&png) {
      Ok(path) => info!("screenshot saved to {}", path.display()),
      Err(e) => error!("screenshot not saved: {e:#}"),
    }
  })
  .detach();
}

fn save(png: &[u8]) -> Result<std::path::PathBuf> {
  let dir = dirs::picture_dir()
    .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
    .context("no pictures directory")?
    .join("Screenshots");
  fs::create_dir_all(&dir)?;

  let time = Zoned::now().strftime("%Y-%m-%d-%H%M%S-%3f");
  let path = dir.join(format!("screenshot-{time}.png"));
  fs::write(&path, png)?;
  Ok(path)
}

pub fn is_empty(b: &Bounds<Pixels>) -> bool {
  b.size.width.as_f32() <= 0. || b.size.height.as_f32() <= 0.
}

fn compose(
  geometry: &HashMap<String, MonitorGeometry>,
  screenshots: &HashMap<String, Frame>,
  area: Bounds<Pixels>,
) -> Result<RgbaImage> {
  let parts = geometry
    .iter()
    .map(|(name, g)| (name, g, area.intersect(&g.bounds())))
    .filter(|(_, _, inter)| !is_empty(inter))
    .collect::<Vec<_>>();
  if parts.is_empty() {
    bail!("selection is outside every monitor");
  }

  let scale = parts.iter().map(|(_, g, _)| g.scale).fold(0., f32::max);
  let len = |v: Pixels, scale: f32| (v.as_f32() * scale).round().max(1.) as u32;
  let (width, height) = (len(area.size.width, scale), len(area.size.height, scale));
  let single = parts.len() == 1;
  let mut out = RgbaImage::new(width, height);

  for (name, g, inter) in parts {
    let Some(shot) = screenshots.get(name) else {
      continue;
    };
    let rel = inter.origin - g.origin;
    let x = (rel.x.as_f32() * g.scale).round() as u32;
    let y = (rel.y.as_f32() * g.scale).round() as u32;
    let (w, h) = (
      len(inter.size.width, g.scale),
      len(inter.size.height, g.scale),
    );
    // Only the selected part leaves the GPU. `read` clamps to the frame, so
    // rounding past its edge is harmless.
    let mut piece = shot.read(x, y, w, h)?;
    let size = (len(inter.size.width, scale), len(inter.size.height, scale));
    if piece.dimensions() != size {
      piece = imageops::resize(&piece, size.0, size.1, FilterType::Nearest);
    }
    if single && piece.dimensions() == (width, height) {
      return Ok(piece);
    }

    let at = inter.origin - area.origin;
    imageops::replace(
      &mut out,
      &piece,
      (at.x.as_f32() * scale).round() as i64,
      (at.y.as_f32() * scale).round() as i64,
    );
  }
  Ok(out)
}

pub fn commit_selection(area: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
  let Some(state) = ScreenshotState::get(cx) else {
    error!("screenshot state missing");
    return;
  };
  let screenshots = std::mem::take(&mut state.screenshots);
  let geometry = state.geometry.clone();
  // Close right away, the readback and encoding happen off the UI thread.
  ScreenshotState::close(Some(window), cx);

  cx.spawn(async move |cx| {
    let png = cx
      .background_executor()
      .spawn(async move { compose(&geometry, &screenshots, area)?.to_png() })
      .await;
    match png {
      Ok(png) => cx.update(|cx| finish(png, cx)),
      Err(e) => error!("screenshot failed: {e:#}"),
    }
  })
  .detach();
}
