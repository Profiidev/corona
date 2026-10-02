use std::{io::Cursor, sync::Arc};

use anyhow::Result;
use gpui_kit::RenderImage;
use image::{Frame, ImageFormat, RgbaImage};

pub trait RgbaImageExt {
  /// For `img()`. gpui keeps decoded images as BGRA.
  fn to_gpui(&self) -> Arc<RenderImage>;
  fn to_png(&self) -> Result<Vec<u8>>;
}

impl RgbaImageExt for RgbaImage {
  fn to_gpui(&self) -> Arc<RenderImage> {
    let mut bgra = self.clone();
    for px in bgra.pixels_mut() {
      px.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(bgra)]))
  }

  fn to_png(&self) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    self.write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
  }
}
