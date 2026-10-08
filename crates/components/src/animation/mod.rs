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

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  const BASE: Duration = Duration::from_millis(300);

  #[gpui::test]
  fn follows_speed_and_reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      assert_eq!(animation_duration(BASE, cx), BASE);

      cx.global_mut::<Config>().shell.animation.speed = 3.;
      assert_eq!(animation_duration(BASE, cx), Duration::from_millis(100));

      cx.global_mut::<Config>().shell.animation.enabled = false;
      assert_eq!(animation_duration(BASE, cx), Duration::ZERO);

      cx.global_mut::<Config>().shell.animation.enabled = true;
      cx.set_reduce_motion(true);
      assert_eq!(animation_duration(BASE, cx), Duration::ZERO);
    });
  }
}
