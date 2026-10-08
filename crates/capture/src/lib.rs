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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn capture_all_fails_when_no_compositor() {
    unsafe {
      std::env::set_var("WAYLAND_DISPLAY", "non-existent-wayland-display");
    }
    let res = capture_all(vec!["DP-1".to_string()]);
    assert!(res.is_err());
  }

  #[test]
  fn capture_all_empty_fails_when_no_compositor() {
    unsafe {
      std::env::set_var("WAYLAND_DISPLAY", "non-existent-wayland-display");
    }
    let res = capture_all(Vec::<String>::new());
    assert!(res.is_err());
  }

  #[test]
  fn capture_all_aborts_on_missing_output() {
    let res = capture_all(vec!["non_existent_output_999".to_string()]);
    assert!(res.is_err());
  }
}
