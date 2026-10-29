pub use corona_utils::level::parse_level;
use gpui_kit::App;

pub mod audio;
pub mod brightness;
pub mod caffeine;
pub mod clipboard;
pub mod dpms;
pub mod extras;
pub mod lock_key;

pub mod monitor;
pub mod network;
pub mod nightlight;
pub mod notification;
pub mod power_profile;
pub mod radio;
pub mod session;
pub mod theme;
pub mod volume;
pub mod window;

pub fn register_commands(cx: &mut App) {
  audio::register_commands(cx);
  brightness::register_commands(cx);
  caffeine::register_commands(cx);
  clipboard::register_commands(cx);
  dpms::register_commands(cx);
  extras::register_commands(cx);
  lock_key::register_commands(cx);
  monitor::register_commands(cx);
  network::register_commands(cx);
  nightlight::register_commands(cx);
  notification::register_commands(cx);
  power_profile::register_commands(cx);
  radio::register_commands(cx);
  session::register_commands(cx);
  theme::register_commands(cx);
  volume::register_commands(cx);
  window::register_commands(cx);
}
