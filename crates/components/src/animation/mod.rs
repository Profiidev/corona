pub mod bounds;
pub mod glide;
pub mod size;
pub mod smooth_retarget;

use std::time::Duration;

use corona_config::ConfigProvider;
use gpui_kit::App;

pub fn animation_duration(base: Duration, cx: &App) -> Duration {
  if cx.reduce_motion() {
    Duration::ZERO
  } else {
    cx.config().shell.animation.duration(base)
  }
}
