use clap::Subcommand;
use clap_complete::CompletionCandidate;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::media::{Action as MediaAction, Media};
use corona_shell::commands::parse_level;
use corona_shell::commands::radio::{Bluetooth, Switch, Wifi};
use corona_shell::commands::session::{Action, Session};
use corona_shell::commands::settings::{Action as SettingsAction, SettingsWindow};
use corona_shell::commands::volume::Device;
use corona_shell::overlays::{
  colorpicker::commands::ColorPicker,
  screenshot::{commands::Screenshot, mode::Mode},
  switcher::commands::{Cycle, Mode as SwitcherMode, Modifier, Options},
};
use corona_shell::settings::PAGES;

use crate::ipc::{
  brightness::BrightnessCommands, notification::NotificationCommands, panel::PanelCommands,
  power_profile::PowerProfileCommands, theme::ThemeCommands, volume::VolumeCommands,
};

mod brightness;
mod notification;
mod panel;
mod power_profile;
mod theme;
mod volume;

#[derive(Subcommand)]
pub enum IpcCommands {
  /// Panel commands
  Panel {
    #[command(subcommand)]
    command: PanelCommands,
  },
  /// Open the screenshot overlay
  Screenshot {
    /// selection, monitor or window
    #[arg(default_value = "selection", ignore_case = true)]
    mode: Mode,
  },
  /// Pick a color from the screen and copy its hex code
  ColorPicker,
  /// Control the active MPRIS player
  Media { action: MediaAction },
  /// Notification commands
  Notification {
    #[command(subcommand)]
    command: NotificationCommands,
  },
  /// Output volume commands
  Volume {
    #[command(subcommand)]
    command: VolumeCommands,
  },
  /// Microphone volume commands
  Mic {
    #[command(subcommand)]
    command: VolumeCommands,
  },
  /// Brightness commands
  Brightness {
    #[command(subcommand)]
    command: BrightnessCommands,
  },
  /// Wi-Fi radio: on, off, toggle or status
  Wifi { switch: Switch },
  /// Bluetooth adapter: on, off, toggle or status
  Bluetooth { switch: Switch },
  /// Power profile commands
  PowerProfile {
    #[command(subcommand)]
    command: PowerProfileCommands,
  },
  /// Theme commands
  Theme {
    #[command(subcommand)]
    command: ThemeCommands,
  },
  /// Lock, suspend, log out, reboot or shut down
  Session { action: Action },
  /// The settings window: open, close or toggle
  Settings {
    action: SettingsAction,
    /// The page to open on
    #[arg(add = clap_complete::ArgValueCandidates::new(|| PAGES.into_iter().map(CompletionCandidate::new).collect::<Vec<_>>()))]
    page: Option<String>,
  },
  /// Open the window switcher, or move its selection while open
  Switcher {
    /// window or workspace
    #[arg(long, default_value = "window", ignore_case = true)]
    mode: SwitcherMode,
    /// Held to keep the switcher open, releasing it switches: super, alt or ctrl
    #[arg(long, default_value = "super", ignore_case = true)]
    modifier: Modifier,
    /// Only show what is on the focused monitor
    #[arg(long)]
    current_monitor: bool,
  },
}

impl IpcCommands {
  pub fn execute(self) {
    match self {
      IpcCommands::Panel { command } => command.execute(),
      IpcCommands::Screenshot { mode } => {
        if let Err(e) = Screenshot::send(mode) {
          tracing::error!("Failed to start screenshot: {}", e);
        }
      }
      IpcCommands::ColorPicker => {
        if let Err(e) = ColorPicker::send(()) {
          tracing::error!("Failed to start color picker: {}", e);
        }
      }
      IpcCommands::Media { action } => {
        if let Err(e) = Media::send(action) {
          tracing::error!("Failed to run media action: {}", e);
        }
      }
      IpcCommands::Notification { command } => command.execute(),
      IpcCommands::Theme { command } => command.execute(),
      IpcCommands::PowerProfile { command } => command.execute(),
      IpcCommands::Wifi { switch } => print_switch(Wifi::send(switch), switch, "Wi-Fi"),
      IpcCommands::Bluetooth { switch } => {
        print_switch(Bluetooth::send(switch), switch, "Bluetooth")
      }
      IpcCommands::Brightness { command } => command.execute(),
      IpcCommands::Volume { command } => command.execute(Device::Output),
      IpcCommands::Mic { command } => command.execute(Device::Input),
      IpcCommands::Settings { action, page } => {
        if let Err(e) = SettingsWindow::send((action, page)) {
          tracing::error!("Failed to open settings: {}", e);
        }
      }
      IpcCommands::Session { action } => {
        if let Err(e) = Session::send(action) {
          tracing::error!("Failed to run session action: {}", e);
        }
      }
      IpcCommands::Switcher {
        mode,
        modifier,
        current_monitor,
      } => {
        let options = Options {
          mode,
          modifier,
          current_monitor,
        };
        if let Err(e) = Cycle::send(options) {
          tracing::error!("Failed to cycle window switcher: {}", e);
        }
      }
    }
  }
}

/// The default volume and brightness step, 5%
const STEP: &str = "5";

fn level(s: &str) -> Result<f32, String> {
  parse_level(s).map_err(|e| e.to_string())
}

/// Completions from a list the running shell sent, none when it is not running
fn candidates<E>(names: Result<Vec<String>, E>) -> Vec<CompletionCandidate> {
  names
    .unwrap_or_default()
    .into_iter()
    .map(CompletionCandidate::new)
    .collect()
}

/// Prints the state for `status`, the error for anything
fn print_switch(result: Result<bool, impl std::fmt::Display>, switch: Switch, what: &str) {
  match result {
    Ok(on) if matches!(switch, Switch::Status) => println!("{}", if on { "on" } else { "off" }),
    Ok(_) => {}
    Err(e) => tracing::error!("Failed to switch {what}: {}", e),
  }
}
