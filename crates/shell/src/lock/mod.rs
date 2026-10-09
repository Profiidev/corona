use corona_auth_screen::User;
use corona_config::ConfigProvider;
use corona_power::PowerExt;
use gpui_kit::{App, AppContext, Task};

pub use state::LockState;

mod state;
mod view;

/// Who is logged in, for the lock screen
fn user(cx: &App) -> User {
  User {
    name: std::env::var("USER").unwrap_or_default().into(),
    avatar: cx
      .config()
      .shell
      .avatar
      .as_deref()
      .map(crate::overlays::wallpaper::source),
  }
}

pub fn init(cx: &mut App) {
  // The lock is usually the first capture, keep the GPU setup out of its delay
  cx.background_spawn(async { corona_capture::warm_up() })
    .detach();
  cx.power().clone().lock_requests(cx, |lock, cx| match lock {
    true => LockState::lock(cx).detach(),
    false => LockState::unlock_animated(cx),
  });
  cx.power()
    .clone()
    .before_sleep(cx, |cx| match cx.config().lockscreen.lock_before_suspend {
      true => LockState::lock(cx),
      false => Task::ready(()),
    });
}
