mod convert;
mod gpu;
mod hyprland;
mod live;
mod toplevel;
mod view;
mod wayland;

use anyhow::Result;
pub use image;

pub use convert::RgbaImageExt;
pub use live::{Frames, capture_window};
pub use view::{FrameView, LiveCapture};
pub use wayland::{Capturer, Frame};

pub fn warm_up() {
  gpu::importable_modifiers();
}

/// Capture all outputs, blocks until all are captured
pub fn capture_all(outputs: impl IntoIterator<Item = String>) -> Result<Vec<(String, Frame)>> {
  let mut capturer = Capturer::new()?;
  let mut last_err = None;
  let frames: Vec<_> = outputs
    .into_iter()
    .filter_map(|name| match capturer.capture(&name) {
      Ok(frame) => Some((name, frame)),
      Err(e) => {
        tracing::warn!("capture of {name} failed: {e:#}");
        last_err = Some(e);
        None
      }
    })
    .collect();
  match last_err {
    Some(e) if frames.is_empty() => Err(e),
    _ => Ok(frames),
  }
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
    unsafe {
      std::env::set_var("WAYLAND_DISPLAY", "non-existent-wayland-display");
    }
    let res = capture_all(vec!["non_existent_output_999".to_string()]);
    assert!(res.is_err());
  }
}
