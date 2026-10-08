use std::collections::HashMap;

use anyhow::Result;
use zbus::{
  Connection,
  fdo::{ManagedObjects, ObjectManagerProxy},
  zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::state::{Adapter, Device, sort_devices};

const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";

pub struct Snapshot {
  pub adapter: Option<Adapter>,
  pub devices: Vec<Device>,
}

pub async fn snapshot(conn: &Connection) -> Result<Snapshot> {
  let objects = ObjectManagerProxy::new(conn, "org.bluez", "/")
    .await?
    .get_managed_objects()
    .await?;
  Ok(read(&objects))
}

fn read(objects: &ManagedObjects) -> Snapshot {
  let interface = |path: &OwnedObjectPath, name: &str| {
    objects
      .get(path)?
      .iter()
      .find(|(interface, _)| interface.as_str() == name)
      .map(|(_, props)| props)
  };

  let mut adapters: Vec<_> = objects
    .keys()
    .filter(|path| interface(path, ADAPTER).is_some())
    .collect();
  adapters.sort_by_key(|path| path.as_str());
  let adapter = adapters.first().and_then(|path| {
    let props = interface(path, ADAPTER)?;
    Some(Adapter {
      path: (*path).clone(),
      name: get::<String>(props, "Alias")
        .or_else(|| get(props, "Name"))
        .unwrap_or_default(),
      powered: get(props, "Powered").unwrap_or(false),
      discoverable: get(props, "Discoverable").unwrap_or(false),
      discovering: get(props, "Discovering").unwrap_or(false),
    })
  });

  let mut devices: Vec<Device> = objects
    .keys()
    .filter_map(|path| {
      let props = interface(path, DEVICE)?;
      let adapter_path = get::<OwnedObjectPath>(props, "Adapter");
      if adapter_path.as_ref() != adapter.as_ref().map(|a| &a.path) {
        return None;
      }
      let address = get::<String>(props, "Address")?;
      Some(Device {
        path: path.clone(),
        name: get(props, "Alias")
          .or_else(|| get(props, "Name"))
          .unwrap_or_else(|| address.clone()),
        address,
        icon: get(props, "Icon"),
        paired: get(props, "Paired").unwrap_or(false),
        connected: get(props, "Connected").unwrap_or(false),
        trusted: get(props, "Trusted").unwrap_or(false),
        battery: interface(path, BATTERY).and_then(|battery| get(battery, "Percentage")),
        rssi: get(props, "RSSI"),
      })
    })
    .collect();
  sort_devices(&mut devices);

  Snapshot { adapter, devices }
}

fn get<T: TryFrom<OwnedValue>>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
  T::try_from(props.get(key)?.try_clone().ok()?).ok()
}

#[cfg(test)]
mod tests {
  use zbus::{names::OwnedInterfaceName, zvariant::Value};

  use super::*;

  type Props = Vec<(&'static str, Value<'static>)>;

  fn objects(entries: Vec<(&str, Vec<(&str, Props)>)>) -> ManagedObjects {
    entries
      .into_iter()
      .map(|(path, interfaces)| {
        let interfaces = interfaces
          .into_iter()
          .map(|(name, props)| {
            let props = props
              .into_iter()
              .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
              .collect();
            (OwnedInterfaceName::try_from(name).unwrap(), props)
          })
          .collect();
        (OwnedObjectPath::try_from(path).unwrap(), interfaces)
      })
      .collect()
  }

  fn path(p: &str) -> Value<'static> {
    Value::from(OwnedObjectPath::try_from(p).unwrap().into_inner())
  }

  fn device(
    adapter: &str,
    address: Option<&'static str>,
    extra: Props,
  ) -> Vec<(&'static str, Props)> {
    let mut props: Props = vec![("Adapter", path(adapter))];
    if let Some(address) = address {
      props.push(("Address", address.into()));
    }
    props.extend(extra);
    vec![(DEVICE, props)]
  }

  #[test]
  fn reads_the_first_adapter_and_its_devices() {
    let snapshot = read(&objects(vec![
      ("/org/bluez", vec![("org.bluez.AgentManager1", vec![])]),
      (
        "/org/bluez/hci1",
        vec![(ADAPTER, vec![("Name", "second".into())])],
      ),
      (
        "/org/bluez/hci0",
        vec![(
          ADAPTER,
          vec![
            ("Alias", "Laptop".into()),
            ("Name", "laptop".into()),
            ("Powered", true.into()),
            ("Discovering", true.into()),
          ],
        )],
      ),
      (
        "/org/bluez/hci0/dev_A",
        device(
          "/org/bluez/hci0",
          Some("AA:AA"),
          vec![
            ("Alias", "Headphones".into()),
            ("Icon", "audio-headphones".into()),
            ("Connected", true.into()),
            ("Paired", true.into()),
            ("Trusted", true.into()),
            ("RSSI", (-40i16).into()),
          ],
        ),
      ),
      (
        "/org/bluez/hci0/dev_B",
        device(
          "/org/bluez/hci0",
          Some("BB:BB"),
          vec![("Name", "Mouse".into())],
        ),
      ),
      // no alias or name: named by address
      (
        "/org/bluez/hci0/dev_C",
        device("/org/bluez/hci0", Some("CC:CC"), vec![]),
      ),
      // no address: skipped
      (
        "/org/bluez/hci0/dev_D",
        device("/org/bluez/hci0", None, vec![]),
      ),
      // another adapter's device
      (
        "/org/bluez/hci1/dev_E",
        device("/org/bluez/hci1", Some("EE:EE"), vec![]),
      ),
    ]));
    let adapter = snapshot.adapter.unwrap();
    assert_eq!(adapter.path.as_str(), "/org/bluez/hci0");
    assert_eq!(adapter.name, "Laptop");
    assert!(adapter.powered && adapter.discovering && !adapter.discoverable);
    let names: Vec<_> = snapshot.devices.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Headphones", "CC:CC", "Mouse"]);
    let headphones = &snapshot.devices[0];
    assert_eq!(headphones.icon.as_deref(), Some("audio-headphones"));
    assert!(headphones.connected && headphones.paired && headphones.trusted);
    assert_eq!((headphones.rssi, headphones.battery), (Some(-40), None));
  }

  #[test]
  fn batteries_and_mistyped_props() {
    let mut with_battery = device(
      "/a",
      Some("AA"),
      vec![("Connected", "yes".into()), ("RSSI", 5u32.into())],
    );
    with_battery.push((BATTERY, vec![("Percentage", 87u8.into())]));
    let snapshot = read(&objects(vec![
      ("/a", vec![(ADAPTER, vec![("Powered", "on".into())])]),
      ("/a/dev", with_battery),
    ]));
    let adapter = snapshot.adapter.unwrap();
    // mistyped values read as their default
    assert_eq!((adapter.name.as_str(), adapter.powered), ("", false));
    let device = &snapshot.devices[0];
    assert_eq!(device.battery, Some(87));
    assert!(!device.connected);
    assert_eq!(device.rssi, None);
  }

  #[test]
  fn no_adapter() {
    let snapshot = read(&ManagedObjects::new());
    assert!(snapshot.adapter.is_none() && snapshot.devices.is_empty());
    // a device of a missing adapter is not shown
    let snapshot = read(&objects(vec![("/a/dev", device("/a", Some("AA"), vec![]))]));
    assert!(snapshot.devices.is_empty());
  }
}
