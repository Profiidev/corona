//! `corona ipc plugin`

use corona_ipc::IpcCommandSend;

use crate::commands::PluginCall;

#[derive(clap::Args)]
pub struct PluginArgs {
  /// The plugin's id
  id: String,
  method: String,
  /// JSON, or else a string
  args: Option<String>,
}

impl PluginArgs {
  pub fn run(self) {
    let PluginArgs { id, method, args } = self;
    let args = args.map_or(serde_json::Value::Null, |args| {
      serde_json::from_str(&args).unwrap_or(serde_json::Value::String(args))
    });
    match PluginCall::send(PluginCall { id, method, args }) {
      Ok(answer) => println!("{answer}"),
      Err(e) => tracing::error!("Failed to call the plugin: {}", e),
    }
  }
}
