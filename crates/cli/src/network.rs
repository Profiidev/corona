use std::{
  net::Ipv4Addr,
  path::{Path, PathBuf},
  process::{Command, Output},
};

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use corona_network_manager::nl80211;

/// where distributions put iptables and nft, pkexec resets PATH
const BIN_DIRS: [&str; 4] = [
  "/run/current-system/sw/bin",
  "/usr/sbin",
  "/usr/bin",
  "/sbin",
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
    /// The host's way to the internet, never the hotspot's way into other networks
    #[arg(long)]
    uplink: Option<String>,
    /// Close the uplink to routing into other networks meanwhile: it did not route before
    #[arg(long)]
    lock_uplink: bool,
  },
  /// Let a hotspot's clients on an existing wifi interface through the firewall, replacing the
  /// rules of an earlier hotspot. Needs root, the shell runs it through pkexec
  AllowHotspot {
    name: String,
    /// The host's way to the internet, never the hotspot's way into other networks
    #[arg(long)]
    uplink: Option<String>,
    /// Close the uplink to routing into other networks meanwhile: it did not route before
    #[arg(long)]
    lock_uplink: bool,
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
        lock_uplink,
      } => nl80211::parse_mac(&mac)
        .with_context(|| format!("{mac:?} is no MAC address"))
        .and_then(|mac| {
          // pkexec lets the session run this without a password: corona's interface, nothing else
          ensure!(
            name == nl80211::AP_INTERFACE,
            "only {} is added",
            nl80211::AP_INTERFACE
          );
          // first, so the interface never runs without
          allow(&name, uplink.as_deref(), lock_uplink)?;
          nl80211::add_ap_interface(phy, &name, mac).inspect_err(|_| {
            // the uplink stays closed to forwarding otherwise
            disallow().ok();
          })
        }),
      NetworkCommands::AllowHotspot {
        name,
        uplink,
        lock_uplink,
      } => {
        // pkexec lets the session run this without a password: no rules for a wired interface
        let wifi = nl80211::valid_interface_name(&name)
          && Path::new("/sys/class/net")
            .join(&name)
            .join("phy80211")
            .exists();
        if wifi {
          allow(&name, uplink.as_deref(), lock_uplink)
        } else {
          Err(anyhow::anyhow!("{name:?} is no wifi interface"))
        }
      }
      NetworkCommands::DisallowHotspot => disallow(),
    };
    if let Err(e) = result {
      eprintln!("error: {e:#}");
      std::process::exit(1);
    }
  }
}

/// the first of `names` found, in that order
fn binary(names: &[&str]) -> Option<PathBuf> {
  names
    .iter()
    .flat_map(|name| BIN_DIRS.map(|dir| Path::new(dir).join(name)))
    .find(|p| p.exists())
}

/// nft first: legacy rules and the system's nftables ruleset cannot see each other
fn iptables() -> Option<PathBuf> {
  binary(&["iptables-nft", "iptables"])
}

fn ip6tables() -> Option<PathBuf> {
  binary(&["ip6tables-nft", "ip6tables"])
}

fn disallow() -> Result<()> {
  if let Some(ip6tables) = ip6tables() {
    disallow_hotspot(&ip6tables)?;
  }
  iptables().map_or(Ok(()), |iptables| disallow_hotspot(&iptables))
}

fn allow(interface: &str, uplink: Option<&str>, lock_uplink: bool) -> Result<()> {
  // iptables reads `+` in a name as a wildcard
  for name in std::iter::once(interface).chain(uplink) {
    if !nl80211::valid_interface_name(name) {
      bail!("{name:?} is no interface name");
    }
  }
  if let Some(uplink) = uplink {
    ensure!(
      Path::new("/sys/class/net").join(uplink).exists(),
      "{uplink} does not exist"
    );
  }
  // without its rules a hotspot's clients reach the host's networks: fail closed
  let iptables = iptables().context("the hotspot needs iptables to keep its clients apart")?;
  disallow_hotspot(&iptables)?;
  // br_netfilter sends bridged frames through FORWARD; they are switched, not routed
  let bridged = Path::new("/proc/sys/net/bridge").exists();
  let routes = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
  allow_hotspot(
    &iptables,
    interface,
    uplink.filter(|_| lock_uplink),
    bridged,
    &direct_networks(&routes, interface),
  )?;
  // the profile disables IPv6, a hotspot started elsewhere may not
  match ip6tables() {
    Some(ip6tables) => {
      disallow_hotspot(&ip6tables)?;
      allow_ipv6(&ip6tables, interface)?;
    }
    None => eprintln!("warning: no ip6tables, IPv6 from the hotspot's clients is not rejected"),
  }
  if let Some(nft) = binary(&["nft"])
    && let Ok(output) = Command::new(nft).args(["list", "ruleset"]).output()
  {
    for table in input_drops(&String::from_utf8_lossy(&output.stdout), interface) {
      eprintln!(
        "warning: nftables table {table} drops input and never names {interface}, the \
         hotspot's clients get no DHCP or DNS: open udp 53, 67 and tcp 53 on {interface} there"
      );
    }
  }
  Ok(())
}

/// what hotspot clients never reach through the host: private, shared (CGNAT), link-local and
/// other special-purpose addresses, the uplink's LAN and VPNs among them
const PRIVATE_NETWORKS: [&str; 8] = [
  "10.0.0.0/8",
  "172.16.0.0/12",
  "192.168.0.0/16",
  "100.64.0.0/10",
  "169.254.0.0/16",
  "0.0.0.0/8",
  "192.0.0.0/24",
  "198.18.0.0/15",
];

/// the networks the host reaches directly, without a gateway, besides the hotspot's own: a LAN or
/// VPN on public addresses, which `PRIVATE_NETWORKS` misses. `routes` is /proc/net/route, its
/// addresses little-endian hex. Only the main table: routes added later, or in other tables, are
/// not seen until the shell runs it again
fn direct_networks(routes: &str, interface: &str) -> Vec<String> {
  let hex = |field: &str| u32::from_str_radix(field, 16).ok();
  routes
    .lines()
    .skip(1)
    .filter_map(|line| {
      let fields: Vec<_> = line.split_whitespace().collect();
      let (dest, gateway, mask) = (
        hex(fields.get(1)?)?,
        hex(fields.get(2)?)?,
        hex(fields.get(7)?)?,
      );
      // a default route without a gateway, a VPN's say, is the internet
      (fields[0] != interface && gateway == 0 && mask != 0).then(|| {
        let dest = Ipv4Addr::from(dest.to_le_bytes());
        format!("{dest}/{}", mask.count_ones())
      })
    })
    .collect()
}

/// lets the hotspot's clients reach NM's dnsmasq (DHCP, DNS) and the internet past firewalls
/// that drop the unknown, like NixOS's input or Docker's forward policy: NM's own accepts sit in
/// their own nftables table, which cannot override a drop in another. A reject here wins over
/// NM's accepts: the host's other services and private networks (the uplink's LAN, VPNs,
/// Docker's) are. The internet is reached the host's way, through a VPN carrying its default
/// route too. NM turns on forwarding for the hotspot, `uplink`, when given, is closed to routing
/// anywhere else meanwhile, Docker's published ports and bridged frames aside. The shell re-runs
/// it when the uplink or the host's networks change
fn allow_hotspot(
  iptables: &Path,
  interface: &str,
  uplink: Option<&str>,
  bridged: bool,
  direct: &[String],
) -> Result<()> {
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
    let mut rule = vec!["FORWARD", "-i", uplink];
    if bridged {
      rule.extend(["-m", "physdev", "!", "--physdev-is-bridged"]);
    }
    rule.extend([
      "-m",
      "conntrack",
      "!",
      "--ctstate",
      "RELATED,ESTABLISHED,DNAT",
    ]);
    rules.push((rule, "REJECT"));
  }
  for network in PRIVATE_NETWORKS
    .iter()
    .copied()
    .chain(direct.iter().map(String::as_str))
  {
    rules.push((vec!["FORWARD", "-i", interface, "-d", network], "REJECT"));
  }
  for (rule, target) in rules {
    insert(iptables, &rule, target)?;
  }
  Ok(())
}

/// rejects all IPv6 from the hotspot's clients, the rules above are IPv4 only
fn allow_ipv6(ip6tables: &Path, interface: &str) -> Result<()> {
  for chain in ["INPUT", "FORWARD"] {
    insert(ip6tables, &[chain, "-i", interface], "REJECT")?;
  }
  Ok(())
}

fn insert(iptables: &Path, rule: &[&str], target: &str) -> Result<()> {
  checked(
    Command::new(iptables)
      .args(["-w", "-I"])
      .args(rule)
      .args(["-m", "comment", "--comment", TAG, "-j", target])
      .output()?,
  )?;
  Ok(())
}

/// the nftables tables besides iptables' own whose input hook drops by default and which never
/// name `interface`: an accept ends evaluation only in its own table, corona's do not get past
/// them, the system firewall's own way to open ports does
fn input_drops(ruleset: &str, interface: &str) -> Vec<String> {
  let named = format!("\"{interface}\"");
  let mut drops = Vec::new();
  let mut table: Option<(String, bool, bool)> = None;
  for line in ruleset.lines() {
    let line = line.trim();
    if let Some(name) = line.strip_prefix("table ") {
      if let Some((name, true, false)) = table.take() {
        drops.push(name);
      }
      let name = name.trim_end_matches('{').trim().to_owned();
      table = Some((name, false, false));
    } else if let Some((name, drops_input, names)) = &mut table {
      *drops_input |= line.contains("hook input") && line.contains("policy drop");
      *names |= line.contains(&named);
      // iptables-nft's tables, corona's rules among them
      if matches!(name.as_str(), "ip filter" | "ip6 filter") {
        *drops_input = false;
      }
    }
  }
  if let Some((name, true, false)) = table {
    drops.push(name);
  }
  drops
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
    allow_hotspot(Path::new("true"), "corona-ap", Some("eth0"), false, &[]).unwrap();
    allow_ipv6(Path::new("true"), "corona-ap").unwrap();
    disallow_hotspot(Path::new("true")).unwrap();
    assert!(allow_hotspot(Path::new("false"), "corona-ap", None, false, &[]).is_err());
    assert!(allow_ipv6(Path::new("false"), "corona-ap").is_err());
  }

  /// the chain's rules after `allow_hotspot`, first checked first
  fn inserted(uplink: Option<&str>, bridged: bool, chain: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("corona-iptables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (fake, log) = (
      dir.join("iptables"),
      dir.join(format!("{chain}-{uplink:?}-{bridged}")),
    );
    // logs the rule
    std::fs::write(
      &fake,
      format!("#!/bin/sh\nshift 2\necho \"$*\" >> '{}'\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let direct = ["203.0.113.0/24".to_owned()];
    allow_hotspot(&fake, "ap0", uplink, bridged, &direct).unwrap();
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
      inserted(Some("eth0"), false, "FORWARD"),
      [
        "FORWARD -i ap0 -d 203.0.113.0/24 -j REJECT",
        "FORWARD -i ap0 -d 198.18.0.0/15 -j REJECT",
        "FORWARD -i ap0 -d 192.0.0.0/24 -j REJECT",
        "FORWARD -i ap0 -d 0.0.0.0/8 -j REJECT",
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
    // bridged frames to VMs and containers are switched, not routed through the uplink
    assert!(
      inserted(Some("eth0"), true, "FORWARD").contains(
        &"FORWARD -i eth0 -m physdev ! --physdev-is-bridged -m conntrack ! --ctstate \
        RELATED,ESTABLISHED,DNAT -j REJECT"
          .to_owned()
      )
    );
    assert_eq!(
      inserted(None, false, "INPUT"),
      [
        "INPUT -i ap0 -p tcp --dport 53 -j ACCEPT",
        "INPUT -i ap0 -p udp -m multiport --dports 53,67 -j ACCEPT",
        "INPUT -i ap0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT",
        "INPUT -i ap0 -j REJECT",
      ]
    );
    // nothing to close without an uplink
    assert!(
      !inserted(None, false, "FORWARD")
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
    assert!(allow("wlan0", Some("eth+"), true).is_err());
    assert!(allow("wlan+", None, false).is_err());
    assert!(allow("wlan0", Some("no-such-uplink"), true).is_err());
  }

  #[test]
  fn directly_connected_networks() {
    let routes = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
      eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
      eth0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n\
      wg0\t007100CB\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n\
      wg0\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\n\
      ap0\t002A0A0A\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n";
    // the gateway routes, the hotspot's own network and a default route without a gateway are not
    assert_eq!(
      direct_networks(routes, "ap0"),
      ["192.168.1.0/24", "203.0.113.0/24"]
    );
  }

  #[test]
  fn foreign_input_drops() {
    let ruleset = "table ip filter {\n\
      \tchain INPUT {\n\t\ttype filter hook input priority filter; policy drop;\n\t}\n}\n\
      table inet nixos-fw {\n\
      \tchain input {\n\t\ttype filter hook input priority filter; policy drop;\n\t}\n}\n\
      table inet open {\n\
      \tchain input {\n\t\ttype filter hook input priority filter; policy drop;\n\
      \t\tiifname \"ap0\" udp dport { 53, 67 } accept\n\t}\n}\n\
      table inet nat {\n\tchain input {\n\t\ttype nat hook input priority 100; policy accept;\n\t}\n}\n";
    assert_eq!(input_drops(ruleset, "ap0"), ["inet nixos-fw"]);
    assert!(input_drops("", "ap0").is_empty());
  }
}
