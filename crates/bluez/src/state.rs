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

  #[test]
  fn sort_edges() {
    let mut devices = vec![
      device("Zed", false, false, None),
      device("weak", false, false, Some(-100)),
      device("alpha", false, false, None),
      device("Beta", false, false, None),
    ];
    sort_devices(&mut devices);
    let names: Vec<_> = devices.iter().map(|d| d.name.as_str()).collect();
    // any signal beats none, then by name ignoring case
    assert_eq!(names, ["weak", "alpha", "Beta", "Zed"]);
    sort_devices(&mut []);
  }

  #[test]
  fn sort_identical_names_and_properties() {
    let mut devices = vec![
      device("beacon", false, false, Some(-60)),
      device("BEACON", false, false, Some(-60)),
    ];
    devices[0].address = "11:11".into();
    devices[1].address = "22:22".into();
    sort_devices(&mut devices);
    assert_eq!(devices[0].address, "11:11");
    assert_eq!(devices[1].address, "22:22");

    let mut reversed = vec![devices[1].clone(), devices[0].clone()];
    sort_devices(&mut reversed);
    // Relative input order is retained without secondary tie-breaker
    assert_eq!(reversed[0].address, "22:22");
    assert_eq!(reversed[1].address, "11:11");
  }
}
