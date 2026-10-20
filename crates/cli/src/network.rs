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

/// marks corona's firewall rules, so they are found again to be removed
const TAG: &str = "corona-hotspot";

#[derive(Subcommand)]
pub enum NetworkCommands {
  /// Add an access point interface on a wifi radio, so a hotspot runs beside its connection, and
  /// let its clients through the firewall. Needs root, the shell runs it through pkexec
  AddApInterface {
    /// The radio's index, N of phyN
    phy: u32,
    name: String,
    mac: String,
    /// The interface the clients reach the internet through, none keeps them on the hotspot
    #[arg(long)]
    uplink: Option<String>,
  },
  /// Let a hotspot's clients on an existing interface through the firewall, replacing the rules
  /// of an earlier hotspot. Needs root, the shell runs it through pkexec
  AllowHotspot {
    name: String,
    /// The interface the clients reach the internet through, none keeps them on the hotspot
    #[arg(long)]
    uplink: Option<String>,
  },
  /// Remove the firewall rules a hotspot added. Needs root, the shell runs it through pkexec
  DisallowHotspot,
}

impl NetworkCommands {
  pub fn execute(self) {
    let result: Result<()> = match self {
      NetworkCommands::AddApInterface {
        phy,
        name,
        mac,
        uplink,
      } => nl80211::parse_mac(&mac)
        .with_context(|| format!("{mac:?} is no MAC address"))
        .and_then(|mac| {
          // first, so the interface never runs without
          allow(&name, uplink.as_deref())?;
          nl80211::add_ap_interface(phy, &name, mac)
        }),
      NetworkCommands::AllowHotspot { name, uplink } => allow(&name, uplink.as_deref()),
      NetworkCommands::DisallowHotspot => iptables().map_or(Ok(()), disallow_hotspot),
    };
    if let Err(e) = result {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }
}

fn iptables() -> Option<&'static Path> {
  IPTABLES.iter().map(Path::new).find(|p| p.exists())
}

fn allow(interface: &str, uplink: Option<&str>) -> Result<()> {
  // iptables reads `+` in a name as a wildcard
  for name in std::iter::once(interface).chain(uplink) {
    if !nl80211::valid_interface_name(name) {
      bail!("{name:?} is no interface name");
    }
  }
  iptables().map_or(Ok(()), |iptables| {
    disallow_hotspot(iptables)?;
    allow_hotspot(iptables, interface, uplink)
  })
}

/// lets the hotspot's clients reach NM's dnsmasq (DHCP, DNS) and `uplink` past firewalls that
/// drop the unknown, like NixOS's input or Docker's forward policy: NM's own accepts sit in their
/// own nftables table, which cannot override a drop in another. Only `uplink`, so the clients stay
/// out of VPNs and Docker's networks. Inserted on top, once.
/// shortcut: forwarding stops when the uplink changes while the hotspot runs, re-run on route
/// changes if that bites
fn allow_hotspot(iptables: &Path, interface: &str, uplink: Option<&str>) -> Result<()> {
  let mut rules: Vec<Vec<&str>> = vec![
    vec![
      "INPUT", "-i", interface, "-p", "udp", "-m", "multiport", "--dports", "53,67",
    ],
    vec!["INPUT", "-i", interface, "-p", "tcp", "--dport", "53"],
  ];
  if let Some(uplink) = uplink {
    rules.push(vec!["FORWARD", "-i", interface, "-o", uplink]);
    rules.push(vec![
      "FORWARD", "-i", uplink, "-o", interface, "-m", "conntrack", "--ctstate",
      "RELATED,ESTABLISHED",
    ]);
  }
  for rule in rules {
    let run = |action| {
      Command::new(iptables)
        .args(["-w", action])
        .args(&rule)
        .args(["-m", "comment", "--comment", TAG, "-j", "ACCEPT"])
        .output()
    };
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

/// removes every rule tagged by `allow_hotspot`, so a stopped hotspot's interface, connected as a
/// client later on, routes nothing into the host's networks
fn disallow_hotspot(iptables: &Path) -> Result<()> {
  for chain in ["INPUT", "FORWARD"] {
    let listed = Command::new(iptables).args(["-w", "-S", chain]).output()?;
    ensure!(
      listed.status.success(),
      "iptables: {}",
      String::from_utf8_lossy(&listed.stderr).trim()
    );
    for rule in tagged(&String::from_utf8_lossy(&listed.stdout)) {
      let output = Command::new(iptables).args(["-w", "-D"]).args(rule).output()?;
      ensure!(
        output.status.success(),
        "iptables: {}",
        String::from_utf8_lossy(&output.stderr).trim()
      );
    }
  }
  Ok(())
}

/// the rules `iptables -S` lists with corona's tag, without their `-A`
fn tagged(listed: &str) -> Vec<Vec<&str>> {
  listed
    .lines()
    .map(|line| line.split_whitespace().collect::<Vec<_>>())
    .filter(|rule| rule.first() == Some(&"-A") && rule.windows(2).any(|w| w == ["--comment", TAG]))
    .map(|rule| rule[1..].to_vec())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn firewall_rules_are_inserted_once() {
    // every rule is there already
    allow_hotspot(Path::new("true"), "corona-ap", Some("eth0")).unwrap();
    disallow_hotspot(Path::new("true")).unwrap();
    // none is, and inserting fails
    assert!(allow_hotspot(Path::new("false"), "corona-ap", None).is_err());
  }

  #[test]
  fn only_tagged_rules_are_removed() {
    let listed = "-P FORWARD DROP\n\
      -A FORWARD -i wlan0 -o eth0 -m comment --comment corona-hotspot -j ACCEPT\n\
      -A FORWARD -i docker0 -j ACCEPT\n\
      -A FORWARD -i eth0 -m comment --comment other -j ACCEPT\n";
    assert_eq!(
      tagged(listed),
      vec![vec![
        "FORWARD", "-i", "wlan0", "-o", "eth0", "-m", "comment", "--comment", "corona-hotspot",
        "-j", "ACCEPT"
      ]]
    );
  }

  #[test]
  fn interface_names_are_checked() {
    assert!(allow("wlan0", Some("eth+")).is_err());
    assert!(allow("wlan+", None).is_err());
  }
}
