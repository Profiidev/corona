use std::{path::Path, process::Command};

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use corona_network_manager::nl80211;

/// where distributions put iptables, pkexec resets PATH
const IPTABLES: [&str; 4] = [
  "/run/current-system/sw/bin/iptables",
  "/usr/sbin/iptables",
  "/usr/bin/iptables",
  "/sbin/iptables",
];

#[derive(Subcommand)]
pub enum NetworkCommands {
  /// Add an access point interface on a wifi radio, so a hotspot runs beside its connection, and
  /// let its clients through the firewall. Needs root, the shell runs it through pkexec
  AddApInterface {
    /// The radio's index, N of phyN
    phy: u32,
    name: String,
    mac: String,
  },
}

impl NetworkCommands {
  pub fn execute(self) {
    let result: Result<()> = match self {
      NetworkCommands::AddApInterface { phy, name, mac } => nl80211::parse_mac(&mac)
        .with_context(|| format!("{mac:?} is no MAC address"))
        .and_then(|mac| {
          // iptables reads `+` in a name as a wildcard
          if !nl80211::valid_interface_name(&name) {
            bail!("{name:?} is no interface name");
          }
          // first, so the interface never runs without
          if let Some(iptables) = IPTABLES.iter().map(Path::new).find(|p| p.exists()) {
            allow_hotspot(iptables, &name)?;
          }
          nl80211::add_ap_interface(phy, &name, mac)
        }),
    };
    if let Err(e) = result {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }
}

/// lets the hotspot's clients reach NM's dnsmasq (DHCP, DNS) and the uplink past firewalls that
/// drop the unknown, like NixOS's input or Docker's forward policy: NM's own accepts sit in their
/// own nftables table, which cannot override a drop in another. Inserted on top, once.
/// shortcut: lost when the firewall reloads (nixos-rebuild switch) until the next boot, re-run on
/// every hotspot start if that bites
fn allow_hotspot(iptables: &Path, interface: &str) -> Result<()> {
  let rules: [&[&str]; 4] = [
    &[
      "INPUT", "-i", interface, "-p", "udp", "-m", "multiport", "--dports", "53,67", "-j", "ACCEPT",
    ],
    &["INPUT", "-i", interface, "-p", "tcp", "--dport", "53", "-j", "ACCEPT"],
    &["FORWARD", "-i", interface, "-j", "ACCEPT"],
    &[
      "FORWARD", "-o", interface, "-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED", "-j",
      "ACCEPT",
    ],
  ];
  for rule in rules {
    let run = |action| Command::new(iptables).args(["-w", action]).args(rule).output();
    if run("-C")?.status.success() {
      continue;
    }
    let output = run("-I")?;
    ensure!(
      output.status.success(),
      "iptables: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn firewall_rules_are_inserted_once() {
    // every rule is there already
    allow_hotspot(Path::new("true"), "corona-ap").unwrap();
    // none is, and inserting fails
    assert!(allow_hotspot(Path::new("false"), "corona-ap").is_err());
  }
}
