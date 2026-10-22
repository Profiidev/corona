//! Each interface's IPv4 forwarding before any hotspot this boot. The kernel forwards what
//! arrives on an interface only with its own `forwarding` on, whether NM flipped the global
//! `ip_forward` (older) or manages it per interface (`ipv4.forwarding`, newer). NM may leave it on
//! after a hotspot, so only the first value seen while none runs counts.

use std::{collections::HashMap, fs, path::PathBuf};

fn read(interface: &str) -> Option<bool> {
  let path = format!("/proc/sys/net/ipv4/conf/{interface}/forwarding");
  Some(fs::read_to_string(path).ok()?.trim() == "1")
}

fn store() -> Option<PathBuf> {
  // tests never touch the home directory
  if cfg!(test) {
    return None;
  }
  let state = std::env::var_os("XDG_STATE_HOME")
    .map(PathBuf::from)
    .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))?;
  Some(state.join("corona/forwarding"))
}

fn boot_id() -> Option<String> {
  Some(
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
      .ok()?
      .trim()
      .to_owned(),
  )
}

/// `boot\n` then `interface 0|1` lines; another boot's is discarded
fn parse(text: &str, boot: &str) -> HashMap<String, bool> {
  let mut lines = text.lines();
  if lines.next() != Some(boot) {
    return HashMap::new();
  }
  lines
    .filter_map(|line| {
      let (interface, on) = line.split_once(' ')?;
      Some((interface.to_owned(), on == "1"))
    })
    .collect()
}

fn load() -> HashMap<String, bool> {
  let (Some(path), Some(boot)) = (store(), boot_id()) else {
    return HashMap::new();
  };
  parse(&fs::read_to_string(path).unwrap_or_default(), &boot)
}

/// the value interfaces created later start with; no interface may be named so
const DEFAULT: &str = "default";

/// records the interfaces not seen yet this boot; call only while no hotspot runs
pub(crate) fn observe<'a>(interfaces: impl Iterator<Item = &'a str>) {
  let mut known = load();
  let (Some(path), Some(boot)) = (store(), boot_id()) else {
    return;
  };
  if !record(&mut known, interfaces, read) {
    return;
  }
  let body: String = known
    .iter()
    .map(|(interface, on)| format!("{interface} {}\n", u8::from(*on)))
    .collect();
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir).ok();
  }
  fs::write(path, format!("{boot}\n{body}")).ok();
}

/// adds the interfaces missing from `known`, true if any. One created after the first observation
/// inherits `default`, which a hotspot NM left forwarding on may have set since: it counts as
/// forwarding only if `default` was on at first
fn record<'a>(
  known: &mut HashMap<String, bool>,
  interfaces: impl Iterator<Item = &'a str>,
  read: impl Fn(&str) -> Option<bool>,
) -> bool {
  let first = !known.contains_key(DEFAULT);
  let mut changed = false;
  // unreadable: off, later interfaces are locked
  if first {
    known.insert(DEFAULT.to_owned(), read(DEFAULT) == Some(true));
    changed = true;
  }
  let inherited = first || known.get(DEFAULT) == Some(&true);
  for interface in interfaces {
    if !known.contains_key(interface)
      && let Some(on) = read(interface)
    {
      known.insert(interface.to_owned(), on && inherited);
      changed = true;
    }
  }
  changed
}

/// the uplink needs closing: it did not route before, or nobody knows
pub(crate) fn lock_needed(uplink: &str) -> bool {
  needs_lock(&load(), uplink)
}

/// one unseen inherits `default`, the value it would have started with before any hotspot
fn needs_lock(known: &HashMap<String, bool>, uplink: &str) -> bool {
  known.get(uplink).or(known.get(DEFAULT)) != Some(&true)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn records_of_this_boot_only() {
    let text = "boot-a\neth0 1\nwlan0 0\nodd\n";
    assert_eq!(
      parse(text, "boot-a"),
      HashMap::from([("eth0".into(), true), ("wlan0".into(), false)])
    );
    assert!(parse(text, "boot-b").is_empty());
    // unknown in tests: always locked
    assert!(lock_needed("eth0"));
  }

  #[test]
  fn unseen_interfaces_inherit_the_default() {
    let known = HashMap::from([(DEFAULT.into(), true), ("eth0".into(), false)]);
    assert!(needs_lock(&known, "eth0"));
    assert!(!needs_lock(&known, "wg0"));
    assert!(needs_lock(&HashMap::from([(DEFAULT.into(), false)]), "wg0"));
    assert!(needs_lock(&HashMap::new(), "wg0"));
  }

  #[test]
  fn later_interfaces_route_only_if_the_default_did_at_first() {
    let mut known = HashMap::new();
    // a hotspot ran since: everything forwards now
    let on = |_: &str| Some(true);
    assert!(record(&mut known, ["eth0"].into_iter(), |i| Some(
      i == "eth0"
    )));
    assert!(record(&mut known, ["eth0", "usb0"].into_iter(), on));
    assert!(!record(&mut known, ["eth0", "usb0"].into_iter(), on));
    assert_eq!(
      known,
      HashMap::from([
        (DEFAULT.into(), false),
        ("eth0".into(), true),
        ("usb0".into(), false)
      ])
    );
    let mut known = HashMap::from([(DEFAULT.into(), true)]);
    record(&mut known, ["usb0"].into_iter(), on);
    assert_eq!(known.get("usb0"), Some(&true));
  }
}
