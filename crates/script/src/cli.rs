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
  pub fn run(self) -> anyhow::Result<()> {
    let PluginArgs { id, method, args } = self;
    let args = args.map_or(serde_json::Value::Null, |args| {
      serde_json::from_str(&args).unwrap_or(serde_json::Value::String(args))
    });
    let answer = PluginCall::send(PluginCall { id, method, args })
      .map_err(|e| anyhow::anyhow!("Failed to call the plugin: {e}"))?;
    println!("{answer}");
    Ok(())
  }
}
