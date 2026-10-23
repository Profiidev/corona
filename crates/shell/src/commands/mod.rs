use corona_ipc::IpcServer;
pub use corona_utils::level::parse_level;

pub mod brightness;
pub mod caffeine;
pub mod dpms;
pub mod lock_key;

pub mod media;
pub mod notification;
pub mod plugin;
pub mod power_profile;
pub mod radio;
pub mod session;
pub mod settings;
pub mod theme;
pub mod volume;

pub fn register_commands(server: &mut IpcServer) {
  brightness::register_commands(server);
  caffeine::register_commands(server);
  dpms::register_commands(server);
  lock_key::register_commands(server);
  media::register_commands(server);
  notification::register_commands(server);
  plugin::register_commands(server);
  power_profile::register_commands(server);
  radio::register_commands(server);
  session::register_commands(server);
  settings::register_commands(server);
  theme::register_commands(server);
  volume::register_commands(server);
}
