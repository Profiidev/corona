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
  /// reads this machine's adapter and devices: `cargo test -p corona_bluez -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn live_snapshot() {
    zbus::block_on(async {
      let conn = zbus::Connection::system().await.unwrap();
      let snapshot = super::snapshot(&conn).await.unwrap();
      println!("{:#?}\n{:#?}", snapshot.adapter, snapshot.devices);
    });
  }
}
