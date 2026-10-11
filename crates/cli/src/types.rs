use std::{fs, path::PathBuf};

use anyhow::Result;
use clap::Args;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::plugin::PluginTypes;

/// `gpui-kit.d.ts` with every corona module, from the running shell
#[derive(Args)]
pub struct TypesCommand {
  /// Write it to this file instead of printing it
  #[arg(short, long)]
  output: Option<PathBuf>,
}

impl TypesCommand {
  pub fn execute(self) {
    if let Err(e) = self.run() {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }

  fn run(self) -> Result<()> {
    let types = PluginTypes::send(())?;
    match self.output {
      Some(path) => fs::write(path, types)?,
      None => print!("{types}"),
    }
    Ok(())
  }
}
