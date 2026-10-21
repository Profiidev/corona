//! Just enough nl80211 to keep wifi connected beside a hotspot: whether a radio serves a station
//! and an access point at once and which clients an access point has, which anyone may ask, and
//! adding the access point's interface, which needs root.

use std::{
  collections::HashMap,
  fs,
  os::fd::OwnedFd,
  path::Path,
  sync::{Mutex, OnceLock},
};

use anyhow::{Context, Result, bail, ensure};
use corona_utils::error::ErrorLogExt;
use rustix::net::{
  AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType,
  netlink::{self, SocketAddrNetlink},
  recv, sendto, socket_with,
};

/// the interface corona adds for its hotspot
pub const AP_INTERFACE: &str = "corona-ap";

// netlink
const NLM_F_REQUEST: u16 = 0x1;
const NLM_F_ACK: u16 = 0x4;
const NLM_F_DUMP: u16 = 0x300;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
/// strips NLA_F_NESTED and NLA_F_NET_BYTEORDER
const NLA_TYPE_MASK: u16 = 0x3fff;
const HEADER: usize = 16;

// generic netlink's controller
const GENL_ID_CTRL: u16 = 16;
const CTRL_CMD_GETFAMILY: u8 = 3;
const CTRL_ATTR_FAMILY_ID: u16 = 1;
const CTRL_ATTR_FAMILY_NAME: u16 = 2;

// nl80211
const CMD_GET_WIPHY: u8 = 1;
const CMD_NEW_INTERFACE: u8 = 7;
const CMD_GET_STATION: u8 = 17;
const ATTR_WIPHY: u16 = 1;
const ATTR_IFINDEX: u16 = 3;
const ATTR_IFNAME: u16 = 4;
const ATTR_IFTYPE: u16 = 5;
const ATTR_MAC: u16 = 6;
const ATTR_INTERFACE_COMBINATIONS: u16 = 120;
const ATTR_SPLIT_WIPHY_DUMP: u16 = 174;
const COMB_LIMITS: u16 = 1;
const COMB_MAXNUM: u16 = 2;
const LIMIT_MAX: u16 = 1;
const LIMIT_TYPES: u16 = 2;
const IFTYPE_STATION: u16 = 2;
const IFTYPE_AP: u16 = 3;

fn align(len: usize) -> usize {
  (len + 3) & !3
}

/// the (type, payload) of each attribute in `data`, stopping at a malformed one
fn attrs(mut data: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
  std::iter::from_fn(move || {
    let len = usize::from(u16::from_ne_bytes([*data.first()?, *data.get(1)?]));
    let kind = u16::from_ne_bytes([*data.get(2)?, *data.get(3)?]) & NLA_TYPE_MASK;
    if len < 4 || len > data.len() {
      return None;
    }
    let payload = &data[4..len];
    data = &data[align(len).min(data.len())..];
    Some((kind, payload))
  })
}

fn u32_payload(payload: &[u8]) -> Option<u32> {
  Some(u32::from_ne_bytes(payload.get(..4)?.try_into().ok()?))
}

fn attr(kind: u16, payload: &[u8]) -> Vec<u8> {
  let len = 4 + payload.len();
  let mut attr = Vec::with_capacity(align(len));
  attr.extend((len as u16).to_ne_bytes());
  attr.extend(kind.to_ne_bytes());
  attr.extend(payload);
  attr.resize(align(len), 0);
  attr
}

fn message(family: u16, flags: u16, cmd: u8, attrs: &[u8]) -> Vec<u8> {
  let len = HEADER + 4 + attrs.len();
  let mut message = Vec::with_capacity(len);
  message.extend((len as u32).to_ne_bytes());
  message.extend(family.to_ne_bytes());
  message.extend((NLM_F_REQUEST | flags).to_ne_bytes());
  // sequence and port, the socket is this request's alone
  message.extend([0; 8]);
  // generic netlink: command, version, reserved
  message.extend([cmd, 1, 0, 0]);
  message.extend(attrs);
  message
}

/// one request on its own socket, `reply` gets each answer's attributes until the kernel is done
fn request(message: &[u8], mut reply: impl FnMut(&[u8])) -> Result<()> {
  let socket: OwnedFd = socket_with(
    AddressFamily::NETLINK,
    SocketType::RAW,
    SocketFlags::CLOEXEC,
    Some(netlink::GENERIC),
  )?;
  sendto(
    &socket,
    message,
    SendFlags::empty(),
    &SocketAddrNetlink::new(0, 0),
  )?;
  let mut buf = vec![0; 32 * 1024];
  loop {
    let (len, _) = recv(&socket, &mut buf[..], RecvFlags::empty())?;
    let mut data = &buf[..len];
    while data.len() >= HEADER {
      let message_len = u32_payload(data).unwrap_or_default() as usize;
      ensure!(
        (HEADER..=data.len()).contains(&message_len),
        "malformed netlink reply"
      );
      let body = &data[HEADER..message_len];
      match u16::from_ne_bytes([data[4], data[5]]) {
        NLMSG_DONE => return Ok(()),
        NLMSG_ERROR => {
          let errno = u32_payload(body).context("malformed netlink error")? as i32;
          // 0 acknowledges the request
          if errno == 0 {
            return Ok(());
          }
          return Err(std::io::Error::from_raw_os_error(-errno).into());
        }
        // past the generic netlink header
        _ => reply(body.get(4..).unwrap_or_default()),
      }
      data = &data[align(message_len).min(data.len())..];
    }
  }
}

fn nl80211() -> Result<u16> {
  let mut family = None;
  let name = attr(CTRL_ATTR_FAMILY_NAME, b"nl80211\0");
  request(
    &message(GENL_ID_CTRL, NLM_F_ACK, CTRL_CMD_GETFAMILY, &name),
    |reply| {
      for (kind, payload) in attrs(reply) {
        if let (CTRL_ATTR_FAMILY_ID, [a, b, ..]) = (kind, payload) {
          family = Some(u16::from_ne_bytes([*a, *b]));
        }
      }
    },
  )?;
  family.context("the kernel has no nl80211")
}

/// one combination has room for a station and an access point
fn station_and_ap(combinations: &[u8]) -> bool {
  attrs(combinations).any(|(_, combination)| {
    let mut total = 0;
    // (max, takes a station, takes an access point)
    let mut limits = Vec::new();
    for (kind, payload) in attrs(combination) {
      match kind {
        COMB_MAXNUM => total = u32_payload(payload).unwrap_or_default(),
        COMB_LIMITS => {
          for (_, limit) in attrs(payload) {
            let (mut max, mut station, mut ap) = (0, false, false);
            for (kind, payload) in attrs(limit) {
              match kind {
                LIMIT_MAX => max = u32_payload(payload).unwrap_or_default(),
                LIMIT_TYPES => {
                  for (iftype, _) in attrs(payload) {
                    station |= iftype == IFTYPE_STATION;
                    ap |= iftype == IFTYPE_AP;
                  }
                }
                _ => {}
              }
            }
            limits.push((max, station, ap));
          }
        }
        _ => {}
      }
    }
    total >= 2
      && limits.iter().enumerate().any(|(i, &(max, station, _))| {
        station
          && max >= 1
          && limits
            .iter()
            .enumerate()
            .any(|(j, &(ap_max, _, ap))| ap && ap_max >= if i == j { 2 } else { 1 })
      })
  })
}

fn query_station_beside_ap(phy: u32) -> Result<bool> {
  let family = nl80211()?;
  let filter = [
    attr(ATTR_WIPHY, &phy.to_ne_bytes()),
    attr(ATTR_SPLIT_WIPHY_DUMP, &[]),
  ]
  .concat();
  let mut fits = false;
  request(
    &message(family, NLM_F_DUMP, CMD_GET_WIPHY, &filter),
    |reply| {
      let attrs: Vec<_> = attrs(reply).collect();
      // a kernel ignoring the filter dumps every radio
      let ours = attrs
        .iter()
        .any(|&(kind, payload)| kind == ATTR_WIPHY && u32_payload(payload) == Some(phy));
      fits |= ours
        && attrs
          .iter()
          .any(|&(kind, payload)| kind == ATTR_INTERFACE_COMBINATIONS && station_and_ap(payload));
    },
  )?;
  Ok(fits)
}

/// radio `phy` runs an access point while its station stays connected; a radio does not change
/// its combinations, so each is asked once; a failed query, a driver still loading say, is retried
pub fn station_beside_ap(phy: u32) -> bool {
  static ASKED: OnceLock<Mutex<HashMap<u32, bool>>> = OnceLock::new();
  let mut asked = ASKED.get_or_init(Default::default).lock().unwrap();
  if let Some(&fits) = asked.get(&phy) {
    return fits;
  }
  let Ok(fits) = query_station_beside_ap(phy).log_err() else {
    return false;
  };
  asked.insert(phy, fits);
  fits
}

pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
  let bytes: Vec<_> = text
    .split(':')
    .map(|byte| (byte.len() == 2).then(|| u8::from_str_radix(byte, 16).ok())?)
    .collect::<Option<_>>()?;
  bytes.try_into().ok()
}

pub fn format_mac(mac: [u8; 6]) -> String {
  mac.map(|byte| format!("{byte:02x}")).join(":")
}

/// a wifi interface's radio: its index and permanent MAC
pub fn radio(interface: &str) -> Option<(u32, [u8; 6])> {
  // tests run against a mock NetworkManager, never this machine's radios
  if cfg!(test) {
    return None;
  }
  let phy = Path::new("/sys/class/net").join(interface).join("phy80211");
  let index = fs::read_to_string(phy.join("index"))
    .ok()?
    .trim()
    .parse()
    .ok()?;
  let mac = parse_mac(fs::read_to_string(phy.join("macaddress")).ok()?.trim())?;
  Some((index, mac))
}

/// a radio's interfaces cannot be up at once with one MAC, the access point's is the radio's,
/// locally administered
pub fn ap_mac(radio: [u8; 6]) -> [u8; 6] {
  let mut mac = radio;
  mac[0] |= 0x02;
  if mac == radio {
    mac[5] ^= 0x01;
  }
  mac
}

/// a station dump reply's MAC
fn station_mac(reply: &[u8]) -> Option<[u8; 6]> {
  attrs(reply).find_map(|(kind, payload)| {
    (kind == ATTR_MAC)
      .then(|| payload.try_into().ok())
      .flatten()
  })
}

/// the MACs of the stations associated with `interface`, an access point's clients; anyone may ask
pub fn stations(interface: &str) -> Result<Vec<[u8; 6]>> {
  let index: u32 = fs::read_to_string(Path::new("/sys/class/net").join(interface).join("ifindex"))
    .with_context(|| format!("{interface} does not exist"))?
    .trim()
    .parse()?;
  let mut stations = Vec::new();
  request(
    &message(
      nl80211()?,
      NLM_F_DUMP,
      CMD_GET_STATION,
      &attr(ATTR_IFINDEX, &index.to_ne_bytes()),
    ),
    |reply| stations.extend(station_mac(reply)),
  )?;
  Ok(stations)
}

pub fn valid_interface_name(name: &str) -> bool {
  (1..16).contains(&name.len())
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// adds an access point interface on radio `phy`, needs root
pub fn add_ap_interface(phy: u32, name: &str, mac: [u8; 6]) -> Result<()> {
  if !valid_interface_name(name) {
    bail!("{name:?} is no interface name");
  }
  let family = nl80211()?;
  let mut name = name.as_bytes().to_vec();
  name.push(0);
  let attrs = [
    attr(ATTR_WIPHY, &phy.to_ne_bytes()),
    attr(ATTR_IFNAME, &name),
    attr(ATTR_IFTYPE, &u32::from(IFTYPE_AP).to_ne_bytes()),
    attr(ATTR_MAC, &mac),
  ]
  .concat();
  request(
    &message(family, NLM_F_ACK, CMD_NEW_INTERFACE, &attrs),
    |_| {},
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn nested(kind: u16, children: &[Vec<u8>]) -> Vec<u8> {
    attr(kind, &children.concat())
  }

  fn limit(max: u32, types: &[u16]) -> Vec<u8> {
    let types: Vec<_> = types.iter().map(|t| attr(*t, &[])).collect();
    nested(
      1,
      &[
        attr(LIMIT_MAX, &max.to_ne_bytes()),
        nested(LIMIT_TYPES, &types),
      ],
    )
  }

  fn combination(total: u32, limits: &[Vec<u8>]) -> Vec<u8> {
    nested(
      1,
      &[
        nested(COMB_LIMITS, limits),
        attr(COMB_MAXNUM, &total.to_ne_bytes()),
      ],
    )
  }

  #[test]
  fn combinations() {
    const P2P_GO: u16 = 9;
    // MT7922: #{ managed } <= 2, #{ P2P-GO } <= 1, total <= 3 and #{ managed } <= 2, #{ AP } <= 1
    let mt7922 = [
      combination(3, &[limit(2, &[IFTYPE_STATION]), limit(1, &[P2P_GO])]),
      combination(3, &[limit(2, &[IFTYPE_STATION]), limit(1, &[IFTYPE_AP])]),
    ]
    .concat();
    assert!(station_and_ap(&mt7922));
    // a station and a P2P group owner, no access point
    assert!(!station_and_ap(&mt7922[..mt7922.len() / 2]));
    // one interface at a time
    let single = combination(1, &[limit(1, &[IFTYPE_STATION, IFTYPE_AP])]);
    assert!(!station_and_ap(&single));
    // both from one shared limit, which has to hold two
    let shared = combination(2, &[limit(2, &[IFTYPE_STATION, IFTYPE_AP])]);
    assert!(station_and_ap(&shared));
    let too_small = combination(2, &[limit(1, &[IFTYPE_STATION, IFTYPE_AP])]);
    assert!(!station_and_ap(&too_small));
    assert!(!station_and_ap(&[]));
    // a cut off attribute ends the list instead of panicking
    assert!(!station_and_ap(&mt7922[..7]));
  }

  #[test]
  fn attributes_round_trip() {
    let data = [
      attr(ATTR_IFNAME, b"wlan0\0"),
      attr(ATTR_WIPHY, &7u32.to_ne_bytes()),
    ]
    .concat();
    let parsed: Vec<_> = attrs(&data).collect();
    assert_eq!(
      parsed,
      [
        (ATTR_IFNAME, &b"wlan0\0"[..]),
        (ATTR_WIPHY, &7u32.to_ne_bytes()[..])
      ]
    );
    // nested flags are masked off the type
    let flagged = attr(ATTR_INTERFACE_COMBINATIONS | 0x8000, &[]);
    assert_eq!(
      attrs(&flagged).next(),
      Some((ATTR_INTERFACE_COMBINATIONS, &[][..]))
    );
    let request = message(0x1c, NLM_F_ACK, CMD_NEW_INTERFACE, &data);
    assert_eq!(request.len(), HEADER + 4 + data.len());
    assert_eq!(u32_payload(&request), Some(request.len() as u32));
  }

  #[test]
  fn macs() {
    assert_eq!(
      parse_mac("14:ac:60:6e:b8:b5"),
      Some([0x14, 0xac, 0x60, 0x6e, 0xb8, 0xb5])
    );
    for bad in [
      "",
      "14:ac:60:6e:b8",
      "14:ac:60:6e:b8:b5:00",
      "1:ac:60:6e:b8:b5",
      "zz:ac:60:6e:b8:b5",
    ] {
      assert_eq!(parse_mac(bad), None, "{bad}");
    }
    assert_eq!(
      format_mac([0x16, 0xac, 0x60, 0x6e, 0xb8, 0xb5]),
      "16:ac:60:6e:b8:b5"
    );
    assert_eq!(
      ap_mac([0x14, 0xac, 0x60, 0x6e, 0xb8, 0xb5]),
      [0x16, 0xac, 0x60, 0x6e, 0xb8, 0xb5]
    );
    // already locally administered: another one
    assert_eq!(
      ap_mac([0x16, 0xac, 0x60, 0x6e, 0xb8, 0xb5]),
      [0x16, 0xac, 0x60, 0x6e, 0xb8, 0xb4]
    );
  }

  #[test]
  fn station_macs() {
    const ATTR_STA_INFO: u16 = 21;
    let mac = [0x5e, 0x0b, 0x2a, 0x11, 0x42, 0x07];
    // as the kernel dumps a station: its interface, MAC and nested statistics
    let reply = [
      attr(ATTR_IFINDEX, &9u32.to_ne_bytes()),
      attr(ATTR_MAC, &mac),
      attr(ATTR_STA_INFO | 0x8000, &attr(1, &0u32.to_ne_bytes())),
    ]
    .concat();
    assert_eq!(station_mac(&reply), Some(mac));
    assert_eq!(station_mac(&reply[..8]), None);
    assert_eq!(station_mac(&attr(ATTR_MAC, &mac[..5])), None);
  }

  #[test]
  fn interface_names() {
    assert!(valid_interface_name(AP_INTERFACE));
    for bad in ["", "a/b", "../x", "with space", "sixteen-chars-xx"] {
      assert!(!valid_interface_name(bad), "{bad}");
    }
    assert!(add_ap_interface(0, "a/b", [0; 6]).is_err());
  }
}
