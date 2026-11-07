use clap::Subcommand;

/// One subcommand per integration, each crate owns its subtree
#[derive(Subcommand)]
pub enum IpcCommands {
  /// Window commands: toggle, move, layout, close all, show desktop, switcher
  Window {
    #[command(subcommand)]
    command: corona_windows::cli::WindowCommands,
  },
  /// Monitor commands: focus, configure, dpms
  Monitor {
    #[command(subcommand)]
    command: corona_windows::cli::MonitorCommands,
  },
  /// Keyboard layouts and lock keys
  Keyboard {
    #[command(subcommand)]
    command: corona_windows::cli::KeyboardCommands,
  },
  /// Audio devices, app streams, output and microphone volume
  Audio {
    #[command(subcommand)]
    command: crate::pipewire::Commands,
  },
  /// Wi-Fi radio and networks, VPNs, hotspot and captive portal
  Network {
    #[command(subcommand)]
    command: corona_network_manager::cli::Commands,
  },
  /// The Bluetooth adapter and devices
  Bluetooth {
    #[command(subcommand)]
    command: crate::bluetooth::Commands,
  },
  /// The active media player: play, pause, seek, shuffle, loop, which one
  Media {
    #[command(subcommand)]
    command: crate::mpris::Commands,
  },
  /// The clipboard history
  Clipboard {
    #[command(subcommand)]
    command: crate::clipboard::Commands,
  },
  /// Removable drives
  Drive {
    #[command(subcommand)]
    command: corona_udisks::cli::Commands,
  },
  /// Kill processes
  Process {
    #[command(subcommand)]
    command: crate::sysinfo::Commands,
  },
  /// Tray items and their menus
  Tray {
    #[command(subcommand)]
    command: crate::tray::Commands,
  },
  /// Open, close or toggle the launcher, the trash, timers and the calculator
  Launcher {
    #[command(subcommand)]
    command: corona_launcher::cli::Commands,
  },
  /// Show, hide or toggle the on-screen keyboard, or set tablet mode
  Osk {
    #[command(subcommand)]
    command: corona_osk::cli::Commands,
  },
  /// Panel commands
  Panel {
    #[command(subcommand)]
    command: corona_surface::cli::PanelCommands,
  },
  /// Open the control center on a page, like audio or network
  ControlCenter(corona_surface::cli::ControlCenterArgs),
  /// Screenshots and the color picker
  Capture {
    #[command(subcommand)]
    command: corona_capture::cli::Commands,
  },
  /// Notification commands
  Notification {
    #[command(subcommand)]
    command: crate::notifications::Commands,
  },
  /// Brightness and nightlight commands
  Brightness {
    #[command(subcommand)]
    command: corona_brightness::cli::Commands,
  },
  /// Keep the session awake
  Idle {
    #[command(subcommand)]
    command: corona_idle::cli::Commands,
  },
  /// Power profile commands
  PowerProfile {
    #[command(subcommand)]
    command: corona_power::cli::Commands,
  },
  /// Theme commands
  Theme {
    #[command(subcommand)]
    command: corona_surface::cli::ThemeCommands,
  },
  /// Call a method of a plugin's service and print its answer as JSON
  Plugin(corona_script::cli::PluginArgs),
  /// Lock, suspend, log out, reboot or shut down
  Session(corona_auth::cli::SessionArgs),
  /// The settings window: open, close or toggle
  Settings(corona_settings::cli::SettingsArgs),
}

impl IpcCommands {
  pub fn execute(self) {
    match self {
      IpcCommands::Window { command } => command.run(),
      IpcCommands::Monitor { command } => command.run(),
      IpcCommands::Keyboard { command } => command.run(),
      IpcCommands::Audio { command } => command.run(),
      IpcCommands::Network { command } => command.run(),
      IpcCommands::Bluetooth { command } => command.run(),
      IpcCommands::Media { command } => command.run(),
      IpcCommands::Clipboard { command } => command.run(),
      IpcCommands::Drive { command } => command.run(),
      IpcCommands::Process { command } => command.run(),
      IpcCommands::Tray { command } => command.run(),
      IpcCommands::Launcher { command } => command.run(),
      IpcCommands::Osk { command } => command.run(),
      IpcCommands::Panel { command } => command.run(),
      IpcCommands::ControlCenter(args) => args.run(),
      IpcCommands::Capture { command } => command.run(),
      IpcCommands::Notification { command } => command.run(),
      IpcCommands::Brightness { command } => command.run(),
      IpcCommands::Idle { command } => command.run(),
      IpcCommands::PowerProfile { command } => command.run(),
      IpcCommands::Theme { command } => command.run(),
      IpcCommands::Plugin(args) => args.run().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
      }),
      IpcCommands::Session(args) => args.run(),
      IpcCommands::Settings(args) => args.run(),
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use clap::Parser;
  use corona_auth::commands::Session;
  use corona_shell::ipc::bluetooth::{Bluetooth, DeviceAction, Discover};
  use corona_brightness::commands::{brightness::SetBrightness, nightlight::Nightlight};
  use corona_capture::commands::{ColorPicker, Screenshot};
  use corona_idle::commands::StayAwake;
  use corona_ipc::{IpcCommand, cli::testing::request};
  use corona_shell::ipc::mpris::media::Media;
  use corona_network_manager::commands::radio::Wifi;
  use corona_shell::ipc::notifications::{ClearHistory, DoNotDisturb, Show};
  use corona_shell::ipc::pipewire::volume::Volume;
  use corona_power::commands::{CycleProfile, SetProfile};
  use corona_script::commands::PluginCall;
  use corona_settings::commands::SettingsWindow;
  use corona_surface::commands::{ClosePanel, OpenPage, OpenPanel, TogglePanel};
  use corona_surface::theme::{SetMode, SetTheme};
  use corona_windows::commands::{dpms::Dpms, lock_key::Pressed, switcher::Cycle};
  use serde_json::{Value, json};

  use super::*;
  use crate::{Cli, Commands};

  fn ipc(args: &[&str]) -> Result<IpcCommands, clap::Error> {
    let cli = Cli::try_parse_from(["corona", "ipc"].iter().chain(args))?;
    match cli.command {
      Commands::Ipc { command } => Ok(command),
      _ => unreachable!(),
    }
  }

  /// Equal, with numbers compared with a tolerance
  fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
      (Value::Number(a), Value::Number(b)) => {
        (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-6
      }
      (Value::Array(a), Value::Array(b)) => {
        a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
      }
      (Value::Object(a), Value::Object(b)) => {
        a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| same(v, w)))
      }
      _ => a == b,
    }
  }

  #[test]
  fn levels() {
    assert!(ipc(&["audio", "volume", "set", "abc"]).is_err());
    assert!(ipc(&["audio", "volume", "set"]).is_err());
  }

  #[test]
  fn value_enums_ignore_case_where_asked() {
    assert!(ipc(&["capture", "screenshot", "MONITOR"]).is_ok());
    assert!(
      ipc(&[
        "window",
        "switcher",
        "--mode",
        "Workspace",
        "--modifier",
        "ALT"
      ])
      .is_ok()
    );
    assert!(ipc(&["capture", "screenshot", "nope"]).is_err());
    assert!(ipc(&["window", "switcher", "--modifier", "meta"]).is_err());
    assert!(ipc(&["network", "wifi", "maybe"]).is_err());
    assert!(ipc(&["brightness", "nightlight", "temp", "warm"]).is_err());
    assert!(ipc(&["brightness", "nightlight", "dim"]).is_err());
    assert!(ipc(&["idle", "stay-awake", "--for", "-5"]).is_err());
    assert!(ipc(&["idle", "stay-awake", "--for", "soon"]).is_err());
  }

  #[test]
  fn sends_what_was_asked() {
    #[rustfmt::skip]
    let cases: &[(&[&str], Value, &str, Value)] = &[
      (&["audio", "volume", "down"], json!(null), Volume::COMMAND, json!(["Output", { "Change": -0.05 }])),
      (&["audio", "volume", "up", "10%"], json!(null), Volume::COMMAND, json!(["Output", { "Change": 0.1 }])),
      (&["audio", "volume", "set", "0.3"], json!(null), Volume::COMMAND, json!(["Output", { "Set": 0.3 }])),
      (&["audio", "mic", "mute"], json!(null), Volume::COMMAND, json!(["Input", "ToggleMute"])),
      (&["audio", "mic", "up"], json!(null), Volume::COMMAND, json!(["Input", { "Change": 0.05 }])),
      (&["brightness", "down", "-m", "*"], json!(null), SetBrightness::COMMAND, json!(["*", { "Change": -0.05 }])),
      (&["brightness", "up"], json!(null), SetBrightness::COMMAND, json!([null, { "Change": 0.05 }])),
      (&["brightness", "set", "40", "--monitor", "DP-1"], json!(null), SetBrightness::COMMAND, json!(["DP-1", { "Set": 0.4 }])),
      (&["brightness", "nightlight", "toggle"], json!({ "on": true, "temperature": 4000 }), Nightlight::COMMAND, json!({ "switch": "Toggle", "temperature": null })),
      (&["brightness", "nightlight", "status"], json!({ "on": false, "temperature": 4000 }), Nightlight::COMMAND, json!({ "switch": "Status", "temperature": null })),
      (&["brightness", "nightlight", "temp", "3200"], json!({ "on": true, "temperature": 3200 }), Nightlight::COMMAND, json!({ "switch": null, "temperature": 3200 })),
      (&["window", "switcher"], json!(null), Cycle::COMMAND, json!({ "mode": "Window", "modifier": "Super", "current_monitor": false })),
      (&["window", "switcher", "--mode", "WORKSPACE", "--modifier", "ctrl", "--current-monitor"], json!(null), Cycle::COMMAND, json!({ "mode": "Workspace", "modifier": "Ctrl", "current_monitor": true })),
      (&["capture", "screenshot"], json!(null), Screenshot::COMMAND, json!("Selection")),
      (&["capture", "screenshot", "Window"], json!(null), Screenshot::COMMAND, json!("Window")),
      (&["capture", "color-picker"], json!(null), ColorPicker::COMMAND, json!(null)),
      (&["media", "next"], json!(null), Media::COMMAND, json!("Next")),
      (&["media", "pause"], json!(null), Media::COMMAND, json!("Pause")),
      (&["settings", "open", "network"], json!(null), SettingsWindow::COMMAND, json!(["Open", "network"])),
      (&["settings", "toggle"], json!(null), SettingsWindow::COMMAND, json!(["Toggle", null])),
      (&["network", "wifi", "status"], json!(true), Wifi::COMMAND, json!("Status")),
      (&["bluetooth", "power", "off"], json!(false), Bluetooth::COMMAND, json!("Off")),
      (&["idle", "stay-awake"], json!(true), StayAwake::COMMAND, json!({ "switch": "Toggle", "minutes": null })),
      (&["idle", "stay-awake", "--for", "30"], json!(true), StayAwake::COMMAND, json!({ "switch": "On", "minutes": 30 })),
      (&["idle", "stay-awake", "toggle", "--for", "60"], json!(false), StayAwake::COMMAND, json!({ "switch": "Toggle", "minutes": 60 })),
      (&["idle", "stay-awake", "status"], json!(false), StayAwake::COMMAND, json!({ "switch": "Status", "minutes": null })),
      (&["monitor", "dpms", "off"], json!(null), Dpms::COMMAND, json!("Off")),
      (&["plugin", "a", "ping"], json!(1), PluginCall::COMMAND, json!({ "id": "a", "method": "ping", "args": null })),
      (&["plugin", "a", "set", "{\"x\": 1}"], json!(null), PluginCall::COMMAND, json!({ "id": "a", "method": "set", "args": { "x": 1 } })),
      (&["plugin", "a", "say", "hi there"], json!(null), PluginCall::COMMAND, json!({ "id": "a", "method": "say", "args": "hi there" })),
      (&["keyboard", "lock-key", "caps"], json!(null), Pressed::COMMAND, json!("Caps")),
      (&["session", "lock-and-suspend"], json!(null), Session::COMMAND, json!("LockAndSuspend")),
      (&["panel", "open", "a"], json!(null), OpenPanel::COMMAND, json!("a")),
      (&["panel", "close", "a"], json!(null), ClosePanel::COMMAND, json!("a")),
      (&["panel", "toggle", "a"], json!(null), TogglePanel::COMMAND, json!("a")),
      (&["notification", "dnd", "status"], json!(true), DoNotDisturb::COMMAND, json!("Status")),
      (&["notification", "dnd", "on"], json!(true), DoNotDisturb::COMMAND, json!("On")),
      (&["notification", "show", "hi"], json!(null), Show::COMMAND, json!("hi")),
      (&["notification", "clear-history"], json!(null), ClearHistory::COMMAND, json!(null)),
      (&["power-profile", "set", "balanced"], json!(null), SetProfile::COMMAND, json!("balanced")),
      (&["power-profile", "cycle"], json!(null), CycleProfile::COMMAND, json!(null)),
      (&["theme", "mode", "get"], json!("dark"), SetMode::COMMAND, json!("Get")),
      (&["theme", "mode", "toggle"], json!("dark"), SetMode::COMMAND, json!("Toggle")),
      (&["theme", "set", "nord"], json!(null), SetTheme::COMMAND, json!("nord")),
      (&["control-center", "audio"], json!(null), OpenPage::COMMAND, json!("audio")),
      (&["window", "toggle", "floating"], json!(null), corona_windows::commands::window::Toggle::COMMAND, json!(["Floating", null])),
      (&["window", "layout", "left-third", "--address", "0xa"], json!(null), corona_windows::commands::window::Place::COMMAND, json!(["LeftThird", "0xa"])),
      (&["window", "move-to-workspace", "3", "--follow"], json!(null), corona_windows::commands::window::MoveToWorkspace::COMMAND, json!({ "address": null, "workspace": "3", "follow": true })),
      (&["window", "close-all", "--keep-focused"], json!(2), corona_windows::commands::window::CloseAll::COMMAND, json!(true)),
      (&["monitor", "focus", "next"], json!(null), corona_windows::commands::window::FocusMonitor::COMMAND, json!("next")),
      (&["monitor", "configure", "DP-1", "mirror", "eDP-1"], json!(null), corona_windows::commands::monitor::Configure::COMMAND, json!(["DP-1", "Mirror", "eDP-1"])),
      (&["window", "show-desktop"], json!(null), corona_windows::commands::window::ShowDesktop::COMMAND, json!(null)),
      (&["keyboard", "layout", "next"], json!(["de", "us"]), corona_windows::commands::window::KeyboardLayout::COMMAND, json!("next")),
      (&["audio", "default", "outputs", "Speakers"], json!(null), corona_shell::ipc::pipewire::audio::SetDefault::COMMAND, json!(["Outputs", "Speakers"])),
      (&["audio", "stream", "firefox", "40"], json!(null), corona_shell::ipc::pipewire::audio::StreamVolume::COMMAND, json!(["firefox", { "Set": 0.4 }])),
      (&["audio", "move", "firefox", "Headphones"], json!(null), corona_shell::ipc::pipewire::audio::MoveStream::COMMAND, json!(["firefox", "Headphones"])),
      (&["network", "connect", "home"], json!(null), corona_network_manager::commands::network::ConnectWifi::COMMAND, json!("home")),
      (&["network", "join-hidden", "x", "--password", "pw"], json!(null), corona_network_manager::commands::network::JoinHidden::COMMAND, json!({ "ssid": "x", "security": "Wpa", "password": "pw" })),
      (&["network", "vpn", "work", "on"], json!(true), corona_network_manager::commands::network::SwitchVpn::COMMAND, json!(["work", "On"])),
      (&["network", "hotspot", "toggle"], json!(true), corona_network_manager::commands::network::Hotspot::COMMAND, json!("Toggle")),
      (&["bluetooth", "device", "connect", "Buds"], json!(null), DeviceAction::COMMAND, json!(["Connect", "Buds"])),
      (&["bluetooth", "discover", "on"], json!(null), Discover::COMMAND, json!(true)),
      (&["media", "seek", "-10"], json!(null), corona_shell::ipc::mpris::player::Seek::COMMAND, json!({ "By": -10.0 })),
      (&["media", "loop", "track"], json!(null), corona_shell::ipc::mpris::player::Loop::COMMAND, json!("Track")),
      (&["tray", "activate", ":1.2/item", "--menu", "4"], json!(null), corona_shell::ipc::tray::Activate::COMMAND, json!([":1.2/item", 4])),
      (&["launcher", "calc", "2", "+", "2"], json!("4"), corona_launcher::ipc::Evaluate::COMMAND, json!("2 + 2")),
      (&["clipboard", "list", "foo", "--limit", "5"], json!([]), corona_shell::ipc::clipboard::ListClipboard::COMMAND, json!(["foo", 5])),
      (&["clipboard", "clear"], json!(3), corona_shell::ipc::clipboard::ClearClipboard::COMMAND, json!(null)),
      (&["launcher", "trash", "count"], json!(3), corona_launcher::ipc::TrashCount::COMMAND, json!(null)),
      (&["drive", "eject", "USB"], json!(null), corona_udisks::commands::DriveAction::COMMAND, json!(["Eject", "USB"])),
      (&["drive", "password-prompt", "--device", "USB (/dev/sdb1)"], json!("pw"), corona_udisks::commands::DrivePasswordPrompt::COMMAND, json!("USB (/dev/sdb1)")),
      (&["process", "kill", "42", "--force"], json!(null), corona_shell::ipc::sysinfo::KillProcess::COMMAND, json!([42, true])),
      (&["process", "kill-port", "8080"], json!([1]), corona_shell::ipc::sysinfo::KillPort::COMMAND, json!(8080)),
      (&["launcher", "timer", "start", "5m", "tea"], json!(1), corona_launcher::ipc::StartTimer::COMMAND, json!({ "seconds": 300, "label": "tea" })),
      (&["launcher", "timer", "stop", "7"], json!(null), corona_launcher::ipc::StopTimer::COMMAND, json!(7)),
      (&["notification", "dismiss"], json!(null), corona_shell::ipc::notifications::DismissToasts::COMMAND, json!(null)),
      (&["launcher", "toggle"], json!(null), corona_launcher::ipc::Toggle::COMMAND, json!({ "command": null, "query": null, "prefill": false })),
      (&["launcher", "close"], json!(null), corona_launcher::ipc::Close::COMMAND, json!(null)),
      (&["launcher", "open", "--command", "volume", "--query", "40"], json!(null), corona_launcher::ipc::Open::COMMAND, json!({ "command": "volume", "query": "40", "prefill": false })),
      (&["launcher", "open"], json!(null), corona_launcher::ipc::Open::COMMAND, json!({ "command": null, "query": null, "prefill": false })),
      (&["osk", "toggle"], json!(null), corona_osk::commands::Toggle::COMMAND, json!(null)),
      (&["osk", "show"], json!(null), corona_osk::commands::Show::COMMAND, json!(null)),
      (&["osk", "tablet", "on"], json!(true), corona_osk::commands::Tablet::COMMAND, json!("On")),
    ];
    for (args, reply, command, data) in cases {
      let cmd = ipc(args).unwrap();
      let got = request(reply.clone(), || cmd.execute());
      assert_eq!(got["command"], *command, "{args:?}");
      assert!(same(&got["data"], data), "{args:?}: {}", got["data"]);
    }
  }

  #[test]
  fn shell_errors_are_not_fatal() {
    // a reply of the wrong type is logged, not a panic
    let got = request(json!("not a bool"), || {
      ipc(&["network", "wifi", "on"]).unwrap().execute()
    });
    assert_eq!(got["command"], Wifi::COMMAND);
    // no shell running at all
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", dir.path()) };
    ipc(&["audio", "volume", "mute"]).unwrap().execute();
  }
}
