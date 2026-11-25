//! `corona ipc capture`

use clap::Subcommand;
use corona_ipc::IpcCommandSend;

use corona_shell::ipc::capture::{
  ColorPicker, ColorPickerStart, Mode, Ruler, RulerMode, RulerStart, Screenshot, ScreenshotStart,
};

#[derive(Subcommand)]
pub enum Commands {
  /// Open the screenshot overlay
  Screenshot {
    /// selection, monitor or window
    #[arg(default_value = "selection", ignore_case = true)]
    mode: Mode,
    /// Keep the screen live instead of freezing it, whatever the config says
    #[arg(long)]
    live: bool,
  },
  /// Pick a color from the screen and copy its hex code
  ColorPicker {
    /// Pick from the live screen instead of a frozen one, whatever the config says
    #[arg(long)]
    live: bool,
  },
  /// Open the screen ruler to measure distances and areas in pixels
  Ruler {
    /// bounds, spacing, horizontal or vertical
    #[arg(default_value = "spacing", ignore_case = true)]
    mode: RulerMode,
    /// Measure the live screen instead of a frozen one, whatever the config says
    #[arg(long)]
    live: bool,
  },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Screenshot { mode, live } => {
        if let Err(e) = Screenshot::send(ScreenshotStart { mode, live }) {
          tracing::error!("Failed to start screenshot: {}", e);
        }
      }
      Commands::ColorPicker { live } => {
        if let Err(e) = ColorPicker::send(ColorPickerStart { live }) {
          tracing::error!("Failed to start color picker: {}", e);
        }
      }
      Commands::Ruler { mode, live } => {
        if let Err(e) = Ruler::send(RulerStart { mode, live }) {
          tracing::error!("Failed to start ruler: {}", e);
        }
      }
    }
  }
}
