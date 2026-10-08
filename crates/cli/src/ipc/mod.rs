use clap::Subcommand;
use clap_complete::CompletionCandidate;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::dpms::{Dpms, Power};
use corona_shell::commands::lock_key::{LockKey, Pressed};
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
  /// Show the OSD of a lock key: caps, num or scroll. Bind it to the key in
  /// Hyprland, e.g. `hl.bind("Caps_Lock", hl.dsp.exec_cmd("corona ipc lock-key caps"))`
  LockKey { key: LockKey },
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
      (&["dpms", "off"], json!(null), Dpms::COMMAND, json!("Off")),
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
