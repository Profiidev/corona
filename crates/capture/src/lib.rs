mod convert;
mod gpu;
mod hyprland;
mod live;
mod view;
mod wayland;

use anyhow::Result;
pub use image;

pub use convert::RgbaImageExt;
pub use live::capture_window;
pub use view::{FrameView, LiveCapture};
pub use wayland::{Capturer, Frame};

/// Capture all outputs, blocks until all are captured
pub fn capture_all(outputs: impl IntoIterator<Item = String>) -> Result<Vec<(String, Frame)>> {
  let mut capturer = Capturer::new()?;
  outputs
    .into_iter()
    .map(|name| capturer.capture(&name).map(|frame| (name, frame)))
    .collect()
}
