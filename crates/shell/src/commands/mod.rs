use corona_ipc::IpcServer;

pub mod media;
pub mod notification;
pub mod session;
pub mod theme;
pub mod volume;

pub fn register_commands(server: &mut IpcServer) {
  media::register_commands(server);
  notification::register_commands(server);
  session::register_commands(server);
  theme::register_commands(server);
  volume::register_commands(server);
}
