//! Completions asked from the running shell; none when it is not running

use clap_complete::CompletionCandidate;
use corona_ipc::IpcCommandSend;
use corona_shell::{
  commands::{
    audio::{Kind, ListNodes},
    bluetooth::ListDevices,
    clipboard::ListClipboard,
    extras::{ListDrives, ListProcesses, ListTimers},
    network::{ListVpns, ListWifi},
    player::ListPlayers,
    tray::ListEntries,
    window::{KeyboardLayout, ListMonitors, ListWindows, ListWorkspaces},
  },
  launcher::commands::ListCommands,
};

/// Values with a description shown beside them
fn described(items: impl IntoIterator<Item = (String, String)>) -> Vec<CompletionCandidate> {
  items
    .into_iter()
    .map(|(value, help)| CompletionCandidate::new(value).help(Some(help.into())))
    .collect()
}

fn plain(items: impl IntoIterator<Item = String>) -> Vec<CompletionCandidate> {
  items.into_iter().map(CompletionCandidate::new).collect()
}

pub fn commands() -> Vec<CompletionCandidate> {
  described(ListCommands::send(()).unwrap_or_default())
}

pub fn windows() -> Vec<CompletionCandidate> {
  described(ListWindows::send(()).unwrap_or_default())
}

pub fn workspaces() -> Vec<CompletionCandidate> {
  described(ListWorkspaces::send(()).unwrap_or_default())
}

pub fn monitors() -> Vec<CompletionCandidate> {
  plain(ListMonitors::send(()).unwrap_or_default())
}

/// `next`, `previous` and every monitor
pub fn monitor_targets() -> Vec<CompletionCandidate> {
  let mut all = plain(["next".to_string(), "previous".to_string()]);
  all.extend(monitors());
  all
}

fn nodes(kind: Kind) -> Vec<CompletionCandidate> {
  described(
    ListNodes::send(kind)
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.name, n.description)),
  )
}

pub fn devices() -> Vec<CompletionCandidate> {
  let mut all = nodes(Kind::Outputs);
  all.extend(nodes(Kind::Inputs));
  all
}

pub fn outputs() -> Vec<CompletionCandidate> {
  nodes(Kind::Outputs)
}

pub fn streams() -> Vec<CompletionCandidate> {
  described(
    ListNodes::send(Kind::Streams)
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.id.to_string(), n.description)),
  )
}

pub fn ssids() -> Vec<CompletionCandidate> {
  described(
    ListWifi::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.ssid, format!("{}% {}", n.strength, n.status))),
  )
}

pub fn vpns() -> Vec<CompletionCandidate> {
  plain(
    ListVpns::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|v| v.name),
  )
}

pub fn bluetooth() -> Vec<CompletionCandidate> {
  described(
    ListDevices::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|d| (d.address, d.name)),
  )
}

pub fn players() -> Vec<CompletionCandidate> {
  plain(ListPlayers::send(()).unwrap_or_default())
}

/// `next`, `previous` and every layout
pub fn layouts() -> Vec<CompletionCandidate> {
  let mut all = plain(["next".to_string(), "previous".to_string()]);
  all.extend(plain(KeyboardLayout::send(None).unwrap_or_default()));
  all
}

pub fn tray() -> Vec<CompletionCandidate> {
  described(
    ListEntries::send(())
      .unwrap_or_default()
      .into_iter()
      .filter(|e| e.menu.is_none())
      .map(|e| (e.address, e.label)),
  )
}

pub fn tray_menus() -> Vec<CompletionCandidate> {
  described(
    ListEntries::send(())
      .unwrap_or_default()
      .into_iter()
      .filter_map(|e| Some((e.menu?.to_string(), e.label))),
  )
}

pub fn clips() -> Vec<CompletionCandidate> {
  described(
    ListClipboard::send((None, 50))
      .unwrap_or_default()
      .into_iter()
      .map(|c| {
        (
          c.id.to_string(),
          c.content.lines().next().unwrap_or_default().to_string(),
        )
      }),
  )
}

pub fn drives() -> Vec<CompletionCandidate> {
  described(
    ListDrives::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|d| (d.device, d.label)),
  )
}

pub fn processes() -> Vec<CompletionCandidate> {
  described(
    ListProcesses::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|(pid, name)| (pid.to_string(), name)),
  )
}

pub fn timers() -> Vec<CompletionCandidate> {
  described(
    ListTimers::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|t| (t.id.to_string(), t.label.unwrap_or_default())),
  )
}
