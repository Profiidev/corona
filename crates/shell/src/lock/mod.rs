use corona_power::PowerExt;
use gpui_kit::App;

pub use state::LockState;

mod state;
mod view;

pub fn init(cx: &mut App) {
  cx.power().clone().before_sleep(cx, LockState::lock);
}
