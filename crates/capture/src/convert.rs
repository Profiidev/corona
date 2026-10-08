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

#[cfg(test)]
mod tests {
  use image::Rgba;

  use super::*;

  fn image() -> RgbaImage {
    RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8, y as u8, 100, 200]))
  }

  #[test]
  fn gpui_images_are_bgra() {
    let original = image();
    let render = original.to_gpui();
    let bytes = render.as_bytes(0).unwrap();
    assert_eq!(&bytes[..8], [100, 0, 0, 200, 100, 0, 1, 200]);
    // the source is left alone
    assert_eq!(original, image());
  }

  #[test]
  fn png_round_trip() {
    let png = image().to_png().unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let back = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(back, image());
    let empty = RgbaImage::new(0, 0).to_png();
    assert!(empty.is_err() || !empty.unwrap().is_empty());
  }
}
