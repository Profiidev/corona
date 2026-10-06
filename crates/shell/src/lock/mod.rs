use corona_config::ConfigProvider;
use corona_power::PowerExt;
use gpui_kit::{App, Task};

pub use state::LockState;

mod state;
mod view;

pub fn init(cx: &mut App) {
  cx.power()
    .clone()
    .before_sleep(cx, |cx| match cx.config().lockscreen.lock_before_suspend {
      true => LockState::lock(cx),
      false => Task::ready(()),
    });
}
