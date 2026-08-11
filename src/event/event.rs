use wayland_client::protocol::wl_output::WlOutput;

#[derive(Clone)]
pub enum ShellEvent {
  // general
  Tick,
  // notifications
  NewNotification {
    id: u32,
    app_name: String,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    timeout_ms: i32,
  },
  CloseNotification(u32),
  // outputs
  NewOutput(WlOutput),
  UpdateOutput(WlOutput),
  DestroyOutput(WlOutput),
}
