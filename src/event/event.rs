pub enum ShellEvent {
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
}
