use std::{
  path::Path,
  process::{Command, Output},
};

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
    /// The host's way to the internet, closed to routing into other networks meanwhile
    #[arg(long)]
    uplink: Option<String>,
  },
  /// Let a hotspot's clients on an existing interface through the firewall, replacing the rules
  /// of an earlier hotspot. Needs root, the shell runs it through pkexec
  AllowHotspot {
    name: String,
    /// The host's way to the internet, closed to routing into other networks meanwhile
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
          nl80211::add_ap_interface(phy, &name, mac).inspect_err(|_| {
            // the uplink stays closed to forwarding otherwise
            if let Some(iptables) = iptables() {
              disallow_hotspot(iptables).ok();
            }
          })
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
  // without its rules a hotspot's clients reach the host's networks: fail closed
  let iptables = iptables().context("the hotspot needs iptables to keep its clients apart")?;
  disallow_hotspot(iptables)?;
  allow_hotspot(iptables, interface, uplink)
}

/// what hotspot clients never reach through the host: private, shared (CGNAT) and link-local
/// addresses, the uplink's LAN and VPNs among them
const PRIVATE_NETWORKS: [&str; 5] = [
  "10.0.0.0/8",
  "172.16.0.0/12",
  "192.168.0.0/16",
  "100.64.0.0/10",
  "169.254.0.0/16",
];

/// lets the hotspot's clients reach NM's dnsmasq (DHCP, DNS) and the internet past firewalls
/// that drop the unknown, like NixOS's input or Docker's forward policy: NM's own accepts sit in
/// their own nftables table, which cannot override a drop in another. A reject here wins over
/// NM's accepts: the host's other services and private networks (the uplink's LAN, VPNs,
/// Docker's) are. The internet is reached the host's way, through a VPN carrying its default
/// route too. NM turns on forwarding for the hotspot, `uplink` is closed to routing anywhere
/// else meanwhile, Docker's published ports aside. The shell re-runs it when the uplink changes
fn allow_hotspot(iptables: &Path, interface: &str, uplink: Option<&str>) -> Result<()> {
  let established = ["-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED"];
  // each is inserted on top: the last one listed is checked first
  let mut rules: Vec<(Vec<&str>, &str)> = vec![
    (vec!["INPUT", "-i", interface], "REJECT"),
    (
      [&["INPUT", "-i", interface][..], &established].concat(),
      "ACCEPT",
    ),
    (
      vec![
        "INPUT",
        "-i",
        interface,
        "-p",
        "udp",
        "-m",
        "multiport",
        "--dports",
        "53,67",
      ],
      "ACCEPT",
    ),
    (
      vec!["INPUT", "-i", interface, "-p", "tcp", "--dport", "53"],
      "ACCEPT",
    ),
    (vec!["FORWARD", "-i", interface], "ACCEPT"),
    (
      [&["FORWARD", "-o", interface][..], &established].concat(),
      "ACCEPT",
    ),
  ];
  if let Some(uplink) = uplink {
    rules.push((
      vec![
        "FORWARD",
        "-i",
        uplink,
        "-m",
        "conntrack",
        "!",
        "--ctstate",
        "RELATED,ESTABLISHED,DNAT",
      ],
      "REJECT",
    ));
  }
  for network in PRIVATE_NETWORKS {
    rules.push((vec!["FORWARD", "-i", interface, "-d", network], "REJECT"));
  }
  for (rule, target) in rules {
    checked(
      Command::new(iptables)
        .args(["-w", "-I"])
        .args(&rule)
        .args(["-m", "comment", "--comment", TAG, "-j", target])
        .output()?,
    )?;
  }
  Ok(())
}

/// removes every rule tagged by `allow_hotspot`, so a stopped hotspot's interface, connected as a
/// client later on, routes nothing into the host's networks
fn disallow_hotspot(iptables: &Path) -> Result<()> {
  for chain in ["INPUT", "FORWARD"] {
    let listed = checked(Command::new(iptables).args(["-w", "-S", chain]).output()?)?;
    for rule in tagged(&String::from_utf8_lossy(&listed.stdout)) {
      checked(
        Command::new(iptables)
          .args(["-w", "-D"])
          .args(rule)
          .output()?,
      )?;
    }
  }
  Ok(())
}

/// `output`, an error with its stderr when iptables failed
fn checked(output: Output) -> Result<Output> {
  ensure!(
    output.status.success(),
    "iptables: {}",
    String::from_utf8_lossy(&output.stderr).trim()
  );
  Ok(output)
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
  fn failing_iptables_is_an_error() {
    allow_hotspot(Path::new("true"), "corona-ap", Some("eth0")).unwrap();
    disallow_hotspot(Path::new("true")).unwrap();
    assert!(allow_hotspot(Path::new("false"), "corona-ap", None).is_err());
  }

  /// the chain's rules after `allow_hotspot`, first checked first
  fn inserted(uplink: Option<&str>, chain: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("corona-iptables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, log) = (
      dir.join("iptables"),
      dir.join(format!("{chain}-{uplink:?}")),
    );
    // logs the rule
    std::fs::write(
      &fake,
      format!("#!/bin/sh\nshift 2\necho \"$*\" >> '{}'\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    allow_hotspot(&fake, "ap0", uplink).unwrap();
    let logged = std::fs::read_to_string(&log).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let rule = |line: &str| line.replace(&format!(" -m comment --comment {TAG}"), "");
    logged
      .lines()
      .rev()
      .filter(|line| line.starts_with(chain))
      .map(rule)
      .collect()
  }

  #[test]
  fn hotspot_clients_reach_only_the_internet_and_dnsmasq() {
    assert_eq!(
      inserted(Some("eth0"), "FORWARD"),
      [
        "FORWARD -i ap0 -d 169.254.0.0/16 -j REJECT",
        "FORWARD -i ap0 -d 100.64.0.0/10 -j REJECT",
        "FORWARD -i ap0 -d 192.168.0.0/16 -j REJECT",
        "FORWARD -i ap0 -d 172.16.0.0/12 -j REJECT",
        "FORWARD -i ap0 -d 10.0.0.0/8 -j REJECT",
        "FORWARD -i eth0 -m conntrack ! --ctstate RELATED,ESTABLISHED,DNAT -j REJECT",
        "FORWARD -o ap0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
        "FORWARD -i ap0 -j ACCEPT",
      ]
    );
    assert_eq!(
      inserted(None, "INPUT"),
      [
        "INPUT -i ap0 -p tcp --dport 53 -j ACCEPT",
        "INPUT -i ap0 -p udp -m multiport --dports 53,67 -j ACCEPT",
        "INPUT -i ap0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
        "INPUT -i ap0 -j REJECT",
      ]
    );
    // nothing to close without an uplink
    assert!(
      !inserted(None, "FORWARD")
        .iter()
        .any(|r| r.contains("! --ctstate"))
    );
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
        "FORWARD",
        "-i",
        "wlan0",
        "-o",
        "eth0",
        "-m",
        "comment",
        "--comment",
        "corona-hotspot",
        "-j",
        "ACCEPT"
      ]]
    );
  }

  #[test]
  fn interface_names_are_checked() {
    assert!(allow("wlan0", Some("eth+")).is_err());
    assert!(allow("wlan+", None).is_err());
  }
}
