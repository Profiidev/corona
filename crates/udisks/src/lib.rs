//! Removable drives through UDisks2: list, mount, unmount and power off

use std::collections::HashMap;

use anyhow::{Context, Result};
use zbus::{
  Connection,
  zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const SERVICE: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

type Properties = HashMap<String, OwnedValue>;
type Objects = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

/// A filesystem on a removable drive
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
  /// The block device's object, what mounting takes
  pub block: OwnedObjectPath,
  pub drive: OwnedObjectPath,
  /// Like `/dev/sdb1`
  pub device: String,
  pub label: String,
  /// The drive's vendor and model
  pub model: String,
  pub size: u64,
  pub mount_points: Vec<String>,
  pub can_power_off: bool,
}

fn get<T: TryFrom<OwnedValue>>(props: &Properties, key: &str) -> Option<T> {
  T::try_from(props.get(key)?.try_clone().ok()?).ok()
}

/// `ay` byte strings, NUL terminated
fn bytes(props: &Properties, key: &str) -> Option<String> {
  let value: &Value = props.get(key)?;
  let bytes: Vec<u8> = match value {
    Value::Array(array) => array
      .iter()
      .filter_map(|v| match v {
        Value::U8(b) => Some(*b),
        _ => None,
      })
      .collect(),
    _ => return None,
  };
  let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
  String::from_utf8(bytes[..end].to_vec()).ok()
}

fn mount_points(props: &Properties) -> Vec<String> {
  let Some(Value::Array(points)) = props.get("MountPoints").map(|v| &**v) else {
    return Vec::new();
  };
  points
    .iter()
    .filter_map(|point| {
      let Value::Array(chars) = point else {
        return None;
      };
      let raw: Vec<u8> = chars
        .iter()
        .filter_map(|v| match v {
          Value::U8(b) => Some(*b),
          _ => None,
        })
        .collect();
      let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
      String::from_utf8(raw[..end].to_vec()).ok()
    })
    .collect()
}

async fn objects(conn: &Connection) -> Result<Objects> {
  let manager = zbus::fdo::ObjectManagerProxy::builder(conn)
    .destination(SERVICE)?
    .path("/org/freedesktop/UDisks2")?
    .build()
    .await?;
  Ok(
    manager
      .get_managed_objects()
      .await?
      .into_iter()
      .map(|(path, interfaces)| {
        let interfaces = interfaces
          .into_iter()
          .map(|(name, props)| {
            (
              name.to_string(),
              props.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            )
          })
          .collect();
        (path, interfaces)
      })
      .collect(),
  )
}

/// Filesystems on removable or ejectable drives
pub async fn volumes(conn: &Connection) -> Result<Vec<Volume>> {
  let objects = objects(conn).await?;
  let mut volumes: Vec<Volume> = objects
    .iter()
    .filter_map(|(path, interfaces)| {
      let block = interfaces.get(BLOCK)?;
      let filesystem = interfaces.get(FILESYSTEM)?;
      let drive_path: OwnedObjectPath = get(block, "Drive")?;
      let drive = objects.get(&drive_path)?.get(DRIVE)?;
      let removable = get::<bool>(drive, "Removable").unwrap_or(false)
        || get::<bool>(drive, "Ejectable").unwrap_or(false)
        || get::<bool>(drive, "MediaRemovable").unwrap_or(false);
      if !removable || get::<bool>(block, "HintIgnore").unwrap_or(false) {
        return None;
      }
      let label = get::<String>(block, "IdLabel").filter(|l| !l.is_empty());
      let vendor = get::<String>(drive, "Vendor").unwrap_or_default();
      let model = get::<String>(drive, "Model").unwrap_or_default();
      let device = bytes(block, "PreferredDevice").unwrap_or_default();
      Some(Volume {
        block: path.clone(),
        drive: drive_path,
        label: label.unwrap_or_else(|| device.clone()),
        device,
        model: format!("{vendor} {model}").trim().to_string(),
        size: get(block, "Size").unwrap_or(0),
        mount_points: mount_points(filesystem),
        can_power_off: get(drive, "CanPowerOff").unwrap_or(false),
      })
    })
    .collect();
  volumes.sort_by(|a, b| a.device.cmp(&b.device));
  Ok(volumes)
}

fn options() -> HashMap<&'static str, Value<'static>> {
  HashMap::new()
}

/// Mounts a volume, returns where
pub async fn mount(conn: &Connection, volume: &Volume) -> Result<String> {
  let reply = conn
    .call_method(
      Some(SERVICE),
      volume.block.as_str(),
      Some(FILESYSTEM),
      "Mount",
      &(options(),),
    )
    .await?;
  Ok(reply.body().deserialize::<String>()?)
}

pub async fn unmount(conn: &Connection, volume: &Volume) -> Result<()> {
  conn
    .call_method(
      Some(SERVICE),
      volume.block.as_str(),
      Some(FILESYSTEM),
      "Unmount",
      &(options(),),
    )
    .await?;
  Ok(())
}

/// Unmounts every volume of the drive and powers it off where it can
pub async fn eject(conn: &Connection, volume: &Volume) -> Result<()> {
  for other in volumes(conn)
    .await?
    .iter()
    .filter(|v| v.drive == volume.drive)
  {
    if !other.mount_points.is_empty() {
      unmount(conn, other)
        .await
        .with_context(|| format!("unmounting {}", other.device))?;
    }
  }
  let method = match volume.can_power_off {
    true => "PowerOff",
    false => "Eject",
  };
  conn
    .call_method(
      Some(SERVICE),
      volume.drive.as_str(),
      Some(DRIVE),
      method,
      &(options(),),
    )
    .await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn byte_strings() {
    let mut props = Properties::new();
    props.insert(
      "PreferredDevice".into(),
      Value::from(b"/dev/sdb1\0".to_vec()).try_into().unwrap(),
    );
    assert_eq!(
      bytes(&props, "PreferredDevice").as_deref(),
      Some("/dev/sdb1")
    );
    let points = vec![b"/run/media/me/USB\0".to_vec()];
    props.insert(
      "MountPoints".into(),
      Value::from(points).try_into().unwrap(),
    );
    assert_eq!(mount_points(&props), ["/run/media/me/USB"]);
  }
}
