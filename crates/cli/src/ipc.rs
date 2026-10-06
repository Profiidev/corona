use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::IpcCommandSend;
use corona_shell::commands::brightness::{Action as BrightnessAction, ListMonitors, SetBrightness};
use corona_shell::commands::media::{Action as MediaAction, Media};
use corona_shell::commands::notification::{ClearHistory, Dnd, DoNotDisturb, Show};
use corona_shell::commands::parse_level;
use corona_shell::commands::session::{Action, Session};
use corona_shell::commands::theme::{ListThemes, Mode as ThemeMode, SetMode, SetTheme};
use corona_shell::commands::volume::{Action as VolumeAction, Device, Volume};
use corona_shell::overlays::{
  colorpicker::commands::ColorPicker,
  screenshot::{commands::Screenshot, mode::Mode},
  switcher::commands::{Cycle, Mode as SwitcherMode, Modifier, Options},
};

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
  /// Theme commands
  Theme {
    #[command(subcommand)]
    command: ThemeCommands,
  },
  /// Lock, suspend, log out, reboot or shut down
  Session { action: Action },
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
      IpcCommands::Brightness { command } => command.execute(),
      IpcCommands::Volume { command } => command.execute(Device::Output),
      IpcCommands::Mic { command } => command.execute(Device::Input),
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

#[derive(Subcommand)]
pub enum PanelCommands {
  /// Open a panel
  Open {
    /// The name of the panel to open
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
  /// Close a panel
  Close {
    /// The name of the panel to close
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
  /// Toggle a panel
  Toggle {
    /// The name of the panel to toggle
    #[arg(add = ArgValueCandidates::new(panel_names))]
    panel: String,
  },
}

fn panel_names() -> Vec<CompletionCandidate> {
  corona_surface::commands::ListPanels::send(())
    .unwrap_or_default()
    .into_iter()
    .map(CompletionCandidate::new)
    .collect()
}

impl PanelCommands {
  pub fn execute(self) {
    match self {
      PanelCommands::Open { panel } => {
        if let Err(e) = corona_surface::commands::OpenPanel::send(panel) {
          tracing::error!("Failed to open panel: {}", e);
        }
      }
      PanelCommands::Close { panel } => {
        if let Err(e) = corona_surface::commands::ClosePanel::send(panel) {
          tracing::error!("Failed to close panel: {}", e);
        }
      }
      PanelCommands::Toggle { panel } => {
        if let Err(e) = corona_surface::commands::TogglePanel::send(panel) {
          tracing::error!("Failed to toggle panel: {}", e);
        }
      }
    }
  }
}

#[derive(Subcommand)]
pub enum NotificationCommands {
  /// Do not disturb: on, off, toggle or status
  Dnd { action: Dnd },
  /// Send a notification
  Show {
    /// The notification's summary
    summary: String,
  },
  /// Remove all entries from the notification history
  ClearHistory,
}

impl NotificationCommands {
  pub fn execute(self) {
    match self {
      NotificationCommands::Dnd { action } => match DoNotDisturb::send(action) {
        Ok(enabled) if matches!(action, Dnd::Status) => {
          println!("{}", if enabled { "on" } else { "off" })
        }
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to set do not disturb: {}", e),
      },
      NotificationCommands::Show { summary } => {
        if let Err(e) = Show::send(summary) {
          tracing::error!("Failed to show notification: {}", e);
        }
      }
      NotificationCommands::ClearHistory => {
        if let Err(e) = ClearHistory::send(()) {
          tracing::error!("Failed to clear notification history: {}", e);
        }
      }
    }
  }
}

#[derive(Subcommand)]
pub enum ThemeCommands {
  /// Dark, light, toggle or get the mode
  Mode { mode: ThemeMode },
  /// Switch to a theme by name
  Set {
    /// The name of the theme
    #[arg(add = ArgValueCandidates::new(theme_names))]
    name: String,
  },
}

/// Asks the running shell, which knows every theme it loaded.
fn theme_names() -> Vec<CompletionCandidate> {
  ListThemes::send(())
    .unwrap_or_default()
    .into_iter()
    .map(CompletionCandidate::new)
    .collect()
}

impl ThemeCommands {
  pub fn execute(self) {
    match self {
      ThemeCommands::Mode { mode } => match SetMode::send(mode) {
        Ok(current) if matches!(mode, ThemeMode::Get) => println!("{current}"),
        Ok(_) => {}
        Err(e) => tracing::error!("Failed to set theme mode: {}", e),
      },
      ThemeCommands::Set { name } => {
        if let Err(e) = SetTheme::send(name) {
          tracing::error!("Failed to set theme: {}", e);
        }
      }
    }
  }
}

const STEP: &str = "5";

fn level(s: &str) -> Result<f32, String> {
  parse_level(s).map_err(|e| e.to_string())
}

#[derive(Subcommand)]
pub enum VolumeCommands {
  /// Set the volume: 65, 65% or 0.65
  Set {
    #[arg(value_parser = level)]
    level: f32,
  },
  /// Raise the volume
  Up {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
  },
  /// Lower the volume
  Down {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
  },
  /// Toggle mute
  Mute,
}

impl VolumeCommands {
  pub fn execute(self, device: Device) {
    let action = match self {
      VolumeCommands::Set { level } => VolumeAction::Set(level),
      VolumeCommands::Up { step } => VolumeAction::Change(step),
      VolumeCommands::Down { step } => VolumeAction::Change(-step),
      VolumeCommands::Mute => VolumeAction::ToggleMute,
    };
    if let Err(e) = Volume::send((device, action)) {
      tracing::error!("Failed to change volume: {}", e);
    }
  }
}

#[derive(clap::Args)]
pub struct MonitorArg {
  /// The monitor's output name, like DP-1, or * for all; the focused one when left out
  #[arg(short, long, add = ArgValueCandidates::new(monitor_names))]
  monitor: Option<String>,
}

/// Asks the running shell for the displays it can dim.
fn monitor_names() -> Vec<CompletionCandidate> {
  ListMonitors::send(())
    .unwrap_or_default()
    .into_iter()
    .chain(["*".to_string()])
    .map(CompletionCandidate::new)
    .collect()
}

#[derive(Subcommand)]
pub enum BrightnessCommands {
  /// Set the brightness: 65, 65% or 0.65
  Set {
    #[arg(value_parser = level)]
    level: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
  /// Raise the brightness
  Up {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
  /// Lower the brightness
  Down {
    /// The step: 10, 10% or 0.1
    #[arg(value_parser = level, default_value = STEP)]
    step: f32,
    #[command(flatten)]
    monitor: MonitorArg,
  },
}

impl BrightnessCommands {
  pub fn execute(self) {
    let (monitor, action) = match self {
      BrightnessCommands::Set { level, monitor } => (monitor, BrightnessAction::Set(level)),
      BrightnessCommands::Up { step, monitor } => (monitor, BrightnessAction::Change(step)),
      BrightnessCommands::Down { step, monitor } => (monitor, BrightnessAction::Change(-step)),
    };
    if let Err(e) = SetBrightness::send((monitor.monitor, action)) {
      tracing::error!("Failed to change brightness: {}", e);
    }
  }
}
