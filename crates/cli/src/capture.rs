//! `corona ipc capture`

use clap::Subcommand;
use corona_ipc::IpcCommandSend;

use corona_shell::ipc::capture::{ColorPicker, Mode, Ruler, RulerMode, Screenshot};

#[derive(Subcommand)]
pub enum Commands {
  /// Open the screenshot overlay
  Screenshot {
    /// selection, monitor or window
    #[arg(default_value = "selection", ignore_case = true)]
    mode: Mode,
  },
  /// Pick a color from the screen and copy its hex code
  ColorPicker,
  /// Open the screen ruler to measure distances and areas in pixels
  Ruler {
    /// bounds, spacing, horizontal or vertical
    #[arg(default_value = "spacing", ignore_case = true)]
    mode: RulerMode,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Screenshot { mode } => {
        if let Err(e) = Screenshot::send(mode) {
          tracing::error!("Failed to start screenshot: {}", e);
        }
      }
      Commands::ColorPicker => {
        if let Err(e) = ColorPicker::send(()) {
          tracing::error!("Failed to start color picker: {}", e);
        }
      }
      Commands::Ruler { mode } => {
        if let Err(e) = Ruler::send(mode) {
          tracing::error!("Failed to start ruler: {}", e);
        }
      }
    }
  }
}
