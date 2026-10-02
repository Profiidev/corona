use std::fs;

use anyhow::{Context, Result, bail};
use corona_capture::{
  RgbaImageExt,
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

use crate::overlays::screenshot::state::ScreenshotState;

fn finish(image: RgbaImage, cx: &mut App) {
  let png = match image.to_png() {
    Ok(png) => png,
    Err(e) => return error!("failed to convert screen shot to png: {e:#}"),
  };

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

  let time = Zoned::now().strftime("%Y-%m-%d-%H%M%S");
  let path = dir.join(format!("screenshot-{time}.png"));
  fs::write(&path, png)?;
  Ok(path)
}

pub fn is_empty(b: &Bounds<Pixels>) -> bool {
  b.size.width.as_f32() <= 0. || b.size.height.as_f32() <= 0.
}

fn compose(state: &ScreenshotState, area: Bounds<Pixels>) -> Result<RgbaImage> {
  let parts = state
    .geometry
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
  let mut out = RgbaImage::new(width, height);

  for (name, g, inter) in parts {
    let Some(shot) = state.screenshots.get(name) else {
      continue;
    };
    let rel = inter.origin - g.origin;
    let x = (rel.x.as_f32() * g.scale).round() as u32;
    let y = (rel.y.as_f32() * g.scale).round() as u32;
    let (w, h) = (
      len(inter.size.width, g.scale),
      len(inter.size.height, g.scale),
    );
    // crop_imm clamps to the frame, so rounding past its edge is harmless.
    let piece = imageops::crop_imm(shot, x, y, w, h).to_image();
    let piece = imageops::resize(
      &piece,
      len(inter.size.width, scale),
      len(inter.size.height, scale),
      FilterType::Nearest,
    );

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
  let Some(state) = cx.try_global::<ScreenshotState>() else {
    error!("screenshot state missing");
    return;
  };
  match compose(state, area) {
    Ok(image) => finish(image, cx),
    Err(e) => error!("screenshot failed: {e:#}"),
  }
  ScreenshotState::close(Some(window), cx);
}
