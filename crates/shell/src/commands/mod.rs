use corona_ipc::IpcServer;
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

pub fn register_commands(server: &mut IpcServer) {
  audio::register_commands(server);
  bluetooth::register_commands(server);
  brightness::register_commands(server);
  caffeine::register_commands(server);
  calculator::register_commands(server);
  clipboard::register_commands(server);
  control_center::register_commands(server);
  dpms::register_commands(server);
  extras::register_commands(server);
  lock_key::register_commands(server);
  media::register_commands(server);
  monitor::register_commands(server);
  network::register_commands(server);
  nightlight::register_commands(server);
  notification::register_commands(server);
  player::register_commands(server);
  plugin::register_commands(server);
  power_profile::register_commands(server);
  radio::register_commands(server);
  session::register_commands(server);
  settings::register_commands(server);
  theme::register_commands(server);
  tray::register_commands(server);
  volume::register_commands(server);
  window::register_commands(server);
}
