use gpui_kit::App;
pub use corona_utils::level::parse_level;

pub mod audio;
pub mod bluetooth;
pub mod brightness;
pub mod caffeine;
pub mod calculator;
pub mod clipboard;
pub mod control_center;
pub mod dpms;
pub mod extras;
pub mod lock_key;

pub mod media;
pub mod monitor;
pub mod network;
pub mod nightlight;
pub mod notification;
pub mod player;
pub mod plugin;
pub mod power_profile;
pub mod radio;
pub mod session;
pub mod settings;
pub mod theme;
pub mod tray;
pub mod volume;
pub mod window;

pub fn register_commands(cx: &mut App) {
  audio::register_commands(cx);
  bluetooth::register_commands(cx);
  brightness::register_commands(cx);
  caffeine::register_commands(cx);
  calculator::register_commands(cx);
  clipboard::register_commands(cx);
  control_center::register_commands(cx);
  dpms::register_commands(cx);
  extras::register_commands(cx);
  lock_key::register_commands(cx);
  media::register_commands(cx);
  monitor::register_commands(cx);
  network::register_commands(cx);
  nightlight::register_commands(cx);
  notification::register_commands(cx);
  player::register_commands(cx);
  plugin::register_commands(cx);
  power_profile::register_commands(cx);
  radio::register_commands(cx);
  session::register_commands(cx);
  settings::register_commands(cx);
  theme::register_commands(cx);
  tray::register_commands(cx);
  volume::register_commands(cx);
  window::register_commands(cx);
}
