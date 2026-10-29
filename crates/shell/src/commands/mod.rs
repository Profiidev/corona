pub use corona_utils::level::parse_level;
use gpui_kit::App;

pub mod caffeine;
pub mod dpms;
pub mod extras;
pub mod lock_key;

pub mod monitor;
pub mod session;
pub mod theme;
pub mod window;

pub fn register_commands(cx: &mut App) {
  caffeine::register_commands(cx);
  dpms::register_commands(cx);
  extras::register_commands(cx);
  lock_key::register_commands(cx);
  monitor::register_commands(cx);
  session::register_commands(cx);
  theme::register_commands(cx);
  window::register_commands(cx);
}
