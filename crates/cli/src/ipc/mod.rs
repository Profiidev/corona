use clap::Subcommand;
use clap_complete::CompletionCandidate;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::bluetooth::{
  Action as BluetoothAction, DeviceAction, Discover, ListDevices as ListBluetooth,
};
use corona_shell::commands::caffeine::{Request as StayAwakeRequest, StayAwake};
use corona_shell::commands::control_center::{ListPages, OpenPage};
use corona_shell::commands::dpms::{Dpms, Power};
use corona_shell::commands::lock_key::{LockKey, Pressed};
use corona_shell::commands::media::{Action as MediaAction, Media};
use corona_shell::commands::parse_level;
use corona_shell::commands::plugin::PluginCall;
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
  audio::AudioCommands,
  brightness::BrightnessCommands,
  clipboard::ClipboardCommands,
  extras::{DriveCommands, ProcessCommands, TimerCommands, TrashCommands},
  launcher::LauncherCommands,
  network::NetworkCommands,
  notification::NotificationCommands,
  panel::PanelCommands,
  player::PlayerCommands,
  power_profile::PowerProfileCommands,
  theme::ThemeCommands,
  tray::TrayCommands,
  volume::VolumeCommands,
  window::{MonitorCommands, WindowCommands},
};

mod audio;
mod brightness;
mod clipboard;
mod extras;
mod launcher;
mod network;
mod notification;
mod panel;
mod player;
mod power_profile;
mod theme;
mod tray;
mod volume;
mod window;

#[derive(Subcommand)]
pub enum IpcCommands {
  /// Window commands: toggle, move, layout, close all
  Window {
    #[command(subcommand)]
    command: WindowCommands,
  },
  /// Monitor commands
  Monitor {
    #[command(subcommand)]
    command: MonitorCommands,
  },
  /// Show the desktop, or the windows again
  ShowDesktop,
  /// List keyboard layouts, or switch to next, previous, an index or a name
  KeyboardLayout { layout: Option<String> },
  /// Audio devices and app streams
  Audio {
    #[command(subcommand)]
    command: AudioCommands,
  },
  /// Wi-Fi networks, VPNs, hotspot and captive portal
  Network {
    #[command(subcommand)]
    command: NetworkCommands,
  },
  /// List Bluetooth devices
  BluetoothDevices,
  /// Connect, disconnect, pair or forget a Bluetooth device by address or name
  BluetoothDevice {
    #[arg(ignore_case = true)]
    action: BluetoothAction,
    device: String,
  },
  /// Look for Bluetooth devices to pair: on or off
  BluetoothDiscover { state: Power },
  /// The active media player: seek, shuffle, loop, which one
  Player {
    #[command(subcommand)]
    command: PlayerCommands,
  },
  /// The clipboard history
  Clipboard {
    #[command(subcommand)]
    command: ClipboardCommands,
  },
  /// The trash
  Trash {
    #[command(subcommand)]
    command: TrashCommands,
  },
  /// Removable drives
  Drive {
    #[command(subcommand)]
    command: DriveCommands,
  },
  /// Kill processes
  Process {
    #[command(subcommand)]
    command: ProcessCommands,
  },
  /// Timers and stopwatches
  Timer {
    #[command(subcommand)]
    command: TimerCommands,
  },
  /// Tray items and their menus
  Tray {
    #[command(subcommand)]
    command: TrayCommands,
  },
  /// Evaluate math, units, currency, time zones or dates like the launcher
  Calc {
    #[arg(allow_hyphen_values = true, num_args = 1.., trailing_var_arg = true)]
    expression: Vec<String>,
  },
  /// Open the control center on a page, like audio or network
  ControlCenter {
    #[arg(add = clap_complete::ArgValueCandidates::new(|| candidates(ListPages::send(()))))]
    page: String,
  },
  /// Open, close or toggle the launcher
  Launcher {
    #[command(subcommand)]
    command: LauncherCommands,
  },
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
  /// Keep the session awake, no idle behaviors and no sleep: on, off, toggle
  /// or status
  StayAwake {
    /// Toggle by default, on with `--for`
    switch: Option<Switch>,
    /// Turn it off again after this many minutes
    #[arg(long = "for", value_name = "MINUTES")]
    minutes: Option<u32>,
  },
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
  /// Show the OSD of a lock key: caps, num or scroll. Bind it to the key in
  /// Hyprland, e.g. `hl.bind("Caps_Lock", hl.dsp.exec_cmd("corona ipc lock-key caps"))`
  LockKey { key: LockKey },
  /// Call a method of a plugin's service and print its answer as JSON
  Plugin {
    /// The plugin's id
    id: String,
    method: String,
    /// JSON, or else a string
    args: Option<String>,
  },
  /// Turn every monitor on or off
  Dpms { power: Power },
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
      IpcCommands::Launcher { command } => command.execute(),
      IpcCommands::Window { command } => command.execute(),
      IpcCommands::Monitor { command } => command.execute(),
      IpcCommands::ShowDesktop => window::show_desktop(),
      IpcCommands::KeyboardLayout { layout } => window::keyboard_layout(layout),
      IpcCommands::Audio { command } => command.execute(),
      IpcCommands::Network { command } => command.execute(),
      IpcCommands::Player { command } => command.execute(),
      IpcCommands::Tray { command } => command.execute(),
      IpcCommands::Clipboard { command } => command.execute(),
      IpcCommands::Trash { command } => command.execute(),
      IpcCommands::Drive { command } => command.execute(),
      IpcCommands::Process { command } => command.execute(),
      IpcCommands::Timer { command } => command.execute(),
      IpcCommands::BluetoothDevices => match ListBluetooth::send(()) {
        Ok(devices) => {
          for d in devices {
            let state = if d.connected {
              "connected"
            } else if d.paired {
              "paired"
            } else {
              ""
            };
            let battery = d.battery.map(|b| format!(" {b}%")).unwrap_or_default();
            println!("{} {state:<9} {}{battery}", d.address, d.name);
          }
        }
        Err(e) => tracing::error!("Failed to list Bluetooth devices: {}", e),
      },
      IpcCommands::BluetoothDevice { action, device } => done(
        DeviceAction::send((action, device)),
        "run the Bluetooth action",
      ),
      IpcCommands::BluetoothDiscover { state } => done(
        Discover::send(matches!(state, Power::On)),
        "switch Bluetooth discovery",
      ),
      IpcCommands::Calc { expression } => {
        match corona_shell::commands::calculator::Evaluate::send(expression.join(" ")) {
          Ok(result) => println!("{result}"),
          Err(e) => tracing::error!("Failed to calculate: {}", e),
        }
      }
      IpcCommands::ControlCenter { page } => {
        if let Err(e) = OpenPage::send(page) {
          tracing::error!("Failed to open the control center: {}", e);
        }
      }
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
      IpcCommands::StayAwake { switch, minutes } => {
        let switch = switch.unwrap_or(match minutes {
          Some(_) => Switch::On,
          None => Switch::Toggle,
        });
        let request = StayAwakeRequest { switch, minutes };
        print_switch(StayAwake::send(request), switch, "stay awake")
      }
      IpcCommands::Brightness { command } => command.execute(),
      IpcCommands::Volume { command } => command.execute(Device::Output),
      IpcCommands::Mic { command } => command.execute(Device::Input),
      IpcCommands::Settings { action, page } => {
        if let Err(e) = SettingsWindow::send((action, page)) {
          tracing::error!("Failed to open settings: {}", e);
        }
      }
      IpcCommands::Plugin { id, method, args } => {
        let args = args.map_or(serde_json::Value::Null, |args| {
          serde_json::from_str(&args).unwrap_or(serde_json::Value::String(args))
        });
        match PluginCall::send(PluginCall { id, method, args }) {
          Ok(answer) => println!("{answer}"),
          Err(e) => tracing::error!("Failed to call the plugin: {}", e),
        }
      }
      IpcCommands::Dpms { power } => {
        if let Err(e) = Dpms::send(power) {
          tracing::error!("Failed to switch the monitors: {}", e);
        }
      }
      IpcCommands::LockKey { key } => {
        if let Err(e) = Pressed::send(key) {
          tracing::error!("Failed to show the lock key: {}", e);
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

/// Logs what failed, if anything did
fn done<T>(result: Result<T, impl std::fmt::Display>, what: &str) {
  if let Err(e) = result {
    tracing::error!("Failed to {what}: {}", e);
  }
}

fn print_lines(lines: Vec<String>) {
  for line in lines {
    println!("{line}");
  }
}

/// Prints the state for `status`, the error for anything
fn print_switch(result: Result<bool, impl std::fmt::Display>, switch: Switch, what: &str) {
  match result {
    Ok(on) if matches!(switch, Switch::Status) => println!("{}", if on { "on" } else { "off" }),
    Ok(_) => {}
    Err(e) => tracing::error!("Failed to switch {what}: {}", e),
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
  };

  use clap::Parser;
  use corona_ipc::IpcCommand;
  use corona_shell::commands::{
    brightness::SetBrightness,
    notification::{ClearHistory, DoNotDisturb, Show},
    power_profile::{CycleProfile, SetProfile},
    theme::{SetMode, SetTheme},
    volume::Volume,
  };
  use corona_surface::commands::{ClosePanel, OpenPanel, TogglePanel};
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

  /// Runs `run` against a fake shell answering `reply`, returns the request it got
  pub(crate) fn request(reply: Value, run: impl FnOnce()) -> Value {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::remove_var("WAYLAND_DISPLAY");
      std::env::remove_var("CORONA_SOCKET");
    }
    let listener = UnixListener::bind(dir.path().join("corona.sock")).unwrap();
    let shell = thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      let mut line = String::new();
      BufReader::new(&stream).read_line(&mut line).unwrap();
      writeln!(stream, "{}", json!({ "Ok": reply })).unwrap();
      serde_json::from_str::<Value>(&line).unwrap()
    });
    run();
    shell.join().unwrap()
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
    for ok in ["65", "65%", "0.65", " 65 % "] {
      assert!((level(ok).unwrap() - 0.65).abs() < 1e-6, "{ok}");
    }
    for bad in ["-1", "abc", "inf", "NaN", ""] {
      assert!(level(bad).is_err(), "{bad}");
    }
    assert!(ipc(&["volume", "set", "abc"]).is_err());
    assert!(ipc(&["volume", "set"]).is_err());
  }

  #[test]
  fn candidates_from_the_shell() {
    let names = candidates::<()>(Ok(vec!["a".into(), "b".into()]));
    let names: Vec<_> = names.iter().map(|c| c.get_value().to_owned()).collect();
    assert_eq!(names, ["a", "b"]);
    // shell not running
    assert!(candidates(Err("down")).is_empty());
  }

  #[test]
  fn value_enums_ignore_case_where_asked() {
    assert!(ipc(&["screenshot", "MONITOR"]).is_ok());
    assert!(ipc(&["switcher", "--mode", "Workspace", "--modifier", "ALT"]).is_ok());
    assert!(ipc(&["screenshot", "nope"]).is_err());
    assert!(ipc(&["switcher", "--modifier", "meta"]).is_err());
    assert!(ipc(&["wifi", "maybe"]).is_err());
    assert!(ipc(&["stay-awake", "--for", "-5"]).is_err());
    assert!(ipc(&["stay-awake", "--for", "soon"]).is_err());
  }

  #[test]
  fn sends_what_was_asked() {
    #[rustfmt::skip]
    let cases: &[(&[&str], Value, &str, Value)] = &[
      (&["volume", "down"], json!(null), Volume::COMMAND, json!(["Output", { "Change": -0.05 }])),
      (&["volume", "up", "10%"], json!(null), Volume::COMMAND, json!(["Output", { "Change": 0.1 }])),
      (&["volume", "set", "0.3"], json!(null), Volume::COMMAND, json!(["Output", { "Set": 0.3 }])),
      (&["mic", "mute"], json!(null), Volume::COMMAND, json!(["Input", "ToggleMute"])),
      (&["mic", "up"], json!(null), Volume::COMMAND, json!(["Input", { "Change": 0.05 }])),
      (&["brightness", "down", "-m", "*"], json!(null), SetBrightness::COMMAND, json!(["*", { "Change": -0.05 }])),
      (&["brightness", "up"], json!(null), SetBrightness::COMMAND, json!([null, { "Change": 0.05 }])),
      (&["brightness", "set", "40", "--monitor", "DP-1"], json!(null), SetBrightness::COMMAND, json!(["DP-1", { "Set": 0.4 }])),
      (&["switcher"], json!(null), Cycle::COMMAND, json!({ "mode": "Window", "modifier": "Super", "current_monitor": false })),
      (&["switcher", "--mode", "WORKSPACE", "--modifier", "ctrl", "--current-monitor"], json!(null), Cycle::COMMAND, json!({ "mode": "Workspace", "modifier": "Ctrl", "current_monitor": true })),
      (&["screenshot"], json!(null), Screenshot::COMMAND, json!("Selection")),
      (&["screenshot", "Window"], json!(null), Screenshot::COMMAND, json!("Window")),
      (&["color-picker"], json!(null), ColorPicker::COMMAND, json!(null)),
      (&["media", "next"], json!(null), Media::COMMAND, json!("Next")),
      (&["settings", "open", "network"], json!(null), SettingsWindow::COMMAND, json!(["Open", "network"])),
      (&["settings", "toggle"], json!(null), SettingsWindow::COMMAND, json!(["Toggle", null])),
      (&["wifi", "status"], json!(true), Wifi::COMMAND, json!("Status")),
      (&["bluetooth", "off"], json!(false), Bluetooth::COMMAND, json!("Off")),
      (&["stay-awake"], json!(true), StayAwake::COMMAND, json!({ "switch": "Toggle", "minutes": null })),
      (&["stay-awake", "--for", "30"], json!(true), StayAwake::COMMAND, json!({ "switch": "On", "minutes": 30 })),
      (&["stay-awake", "toggle", "--for", "60"], json!(false), StayAwake::COMMAND, json!({ "switch": "Toggle", "minutes": 60 })),
      (&["stay-awake", "status"], json!(false), StayAwake::COMMAND, json!({ "switch": "Status", "minutes": null })),
      (&["dpms", "off"], json!(null), Dpms::COMMAND, json!("Off")),
      (&["plugin", "a", "ping"], json!(1), PluginCall::COMMAND, json!({ "id": "a", "method": "ping", "args": null })),
      (&["plugin", "a", "set", "{\"x\": 1}"], json!(null), PluginCall::COMMAND, json!({ "id": "a", "method": "set", "args": { "x": 1 } })),
      (&["plugin", "a", "say", "hi there"], json!(null), PluginCall::COMMAND, json!({ "id": "a", "method": "say", "args": "hi there" })),
      (&["lock-key", "caps"], json!(null), Pressed::COMMAND, json!("Caps")),
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
      (&["window", "toggle", "floating"], json!(null), corona_shell::commands::window::Toggle::COMMAND, json!(["Floating", null])),
      (&["window", "layout", "left-third", "--address", "0xa"], json!(null), corona_shell::commands::window::Place::COMMAND, json!(["LeftThird", "0xa"])),
      (&["window", "move-to-workspace", "3", "--follow"], json!(null), corona_shell::commands::window::MoveToWorkspace::COMMAND, json!({ "address": null, "workspace": "3", "follow": true })),
      (&["window", "close-all", "--keep-focused"], json!(2), corona_shell::commands::window::CloseAll::COMMAND, json!(true)),
      (&["monitor", "focus", "next"], json!(null), corona_shell::commands::window::FocusMonitor::COMMAND, json!("next")),
      (&["monitor", "configure", "DP-1", "mirror", "eDP-1"], json!(null), corona_shell::commands::monitor::Configure::COMMAND, json!(["DP-1", "Mirror", "eDP-1"])),
      (&["show-desktop"], json!(null), corona_shell::commands::window::ShowDesktop::COMMAND, json!(null)),
      (&["keyboard-layout", "next"], json!(["de", "us"]), corona_shell::commands::window::KeyboardLayout::COMMAND, json!("next")),
      (&["audio", "default", "outputs", "Speakers"], json!(null), corona_shell::commands::audio::SetDefault::COMMAND, json!(["Outputs", "Speakers"])),
      (&["audio", "stream", "firefox", "40"], json!(null), corona_shell::commands::audio::StreamVolume::COMMAND, json!(["firefox", { "Set": 0.4 }])),
      (&["audio", "move", "firefox", "Headphones"], json!(null), corona_shell::commands::audio::MoveStream::COMMAND, json!(["firefox", "Headphones"])),
      (&["network", "connect", "home"], json!(null), corona_shell::commands::network::ConnectWifi::COMMAND, json!("home")),
      (&["network", "join-hidden", "x", "--password", "pw"], json!(null), corona_shell::commands::network::JoinHidden::COMMAND, json!({ "ssid": "x", "security": "Wpa", "password": "pw" })),
      (&["network", "vpn", "work", "on"], json!(true), corona_shell::commands::network::SwitchVpn::COMMAND, json!(["work", "On"])),
      (&["network", "hotspot", "toggle"], json!(true), corona_shell::commands::network::Hotspot::COMMAND, json!("Toggle")),
      (&["bluetooth-device", "connect", "Buds"], json!(null), DeviceAction::COMMAND, json!(["Connect", "Buds"])),
      (&["bluetooth-discover", "on"], json!(null), Discover::COMMAND, json!(true)),
      (&["player", "seek", "-10"], json!(null), corona_shell::commands::player::Seek::COMMAND, json!({ "By": -10.0 })),
      (&["player", "loop", "track"], json!(null), corona_shell::commands::player::Loop::COMMAND, json!("Track")),
      (&["tray", "activate", ":1.2/item", "--menu", "4"], json!(null), corona_shell::commands::tray::Activate::COMMAND, json!([":1.2/item", 4])),
      (&["calc", "2", "+", "2"], json!("4"), corona_shell::commands::calculator::Evaluate::COMMAND, json!("2 + 2")),
      (&["clipboard", "list", "foo", "--limit", "5"], json!([]), corona_shell::commands::clipboard::ListClipboard::COMMAND, json!(["foo", 5])),
      (&["clipboard", "clear"], json!(3), corona_shell::commands::clipboard::ClearClipboard::COMMAND, json!(null)),
      (&["trash", "count"], json!(3), corona_shell::commands::extras::TrashCount::COMMAND, json!(null)),
      (&["drive", "eject", "USB"], json!(null), corona_shell::commands::extras::DriveAction::COMMAND, json!(["Eject", "USB"])),
      (&["process", "kill", "42", "--force"], json!(null), corona_shell::commands::extras::KillProcess::COMMAND, json!([42, true])),
      (&["process", "kill-port", "8080"], json!([1]), corona_shell::commands::extras::KillPort::COMMAND, json!(8080)),
      (&["timer", "start", "5m", "tea"], json!(1), corona_shell::commands::extras::StartTimer::COMMAND, json!({ "seconds": 300, "label": "tea" })),
      (&["timer", "stop", "7"], json!(null), corona_shell::commands::extras::StopTimer::COMMAND, json!(7)),
      (&["notification", "dismiss"], json!(null), corona_shell::commands::notification::DismissToasts::COMMAND, json!(null)),
      (&["launcher", "toggle"], json!(null), corona_shell::launcher::commands::Toggle::COMMAND, json!(null)),
      (&["launcher", "close"], json!(null), corona_shell::launcher::commands::Close::COMMAND, json!(null)),
      (&["launcher", "open", "--command", "volume", "--query", "40"], json!(null), corona_shell::launcher::commands::Open::COMMAND, json!({ "command": "volume", "query": "40" })),
      (&["launcher", "open"], json!(null), corona_shell::launcher::commands::Open::COMMAND, json!({ "command": null, "query": null })),
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
      ipc(&["wifi", "on"]).unwrap().execute()
    });
    assert_eq!(got["command"], Wifi::COMMAND);
    // no shell running at all
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", dir.path()) };
    ipc(&["volume", "mute"]).unwrap().execute();
  }
}
