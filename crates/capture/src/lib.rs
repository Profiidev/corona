mod convert;
mod wayland;

use anyhow::Result;
pub use image;
use image::RgbaImage;

pub use convert::RgbaImageExt;
pub use wayland::Capturer;

/// Capture all outputs, blocks until all are captured
pub fn capture_all(outputs: impl IntoIterator<Item = String>) -> Result<Vec<(String, RgbaImage)>> {
  let mut capturer = Capturer::new()?;
  outputs
    .into_iter()
    .map(|name| capturer.capture(&name).map(|image| (name, image)))
    .collect()
}
