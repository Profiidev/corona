use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, PartialEq)]
pub struct Adapter {
  pub path: OwnedObjectPath,
  pub name: String,
  pub powered: bool,
  pub discoverable: bool,
  pub discovering: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
  pub path: OwnedObjectPath,
  pub address: String,
  pub name: String,
  pub icon: Option<String>,
  pub paired: bool,
  pub connected: bool,
  pub trusted: bool,
  pub battery: Option<u8>,
  pub rssi: Option<i16>,
}

pub(crate) fn sort_devices(devices: &mut [Device]) {
  devices.sort_by(|a, b| {
    b.connected
      .cmp(&a.connected)
      .then(b.paired.cmp(&a.paired))
      .then(b.rssi.cmp(&a.rssi))
      .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn device(name: &str, connected: bool, paired: bool, rssi: Option<i16>) -> Device {
    Device {
      path: OwnedObjectPath::default(),
      address: name.into(),
      name: name.into(),
      icon: None,
      paired,
      connected,
      trusted: false,
      battery: None,
      rssi,
    }
  }

  #[test]
  fn sort() {
    let mut devices = vec![
      device("far", false, false, Some(-90)),
      device("b paired", false, true, None),
      device("near", false, false, Some(-40)),
      device("connected", true, true, None),
      device("A paired", false, true, None),
    ];
    sort_devices(&mut devices);
    let names: Vec<_> = devices.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["connected", "A paired", "b paired", "near", "far"]);
  }
}
