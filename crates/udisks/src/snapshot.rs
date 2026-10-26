use std::collections::HashMap;

use anyhow::Result;
use zbus::{
  Connection,
  fdo::{ManagedObjects, ObjectManagerProxy, PropertiesProxy},
  names::{InterfaceName, OwnedInterfaceName},
  zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::{
  SERVICE,
  state::{
    Ata, Block, Drive, Encrypted, Filesystem, Job, Lock, Loop, NvmeController, NvmeHealth,
    Partition, PartitionTable, Swapspace, Volume, sort_volumes,
  },
};

const ROOT: &str = "/org/freedesktop/UDisks2";

const MANAGER: &str = "org.freedesktop.UDisks2.Manager";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const ATA: &str = "org.freedesktop.UDisks2.Drive.Ata";
const NVME_CONTROLLER: &str = "org.freedesktop.UDisks2.NVMe.Controller";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const PARTITION: &str = "org.freedesktop.UDisks2.Partition";
const PARTITION_TABLE: &str = "org.freedesktop.UDisks2.PartitionTable";
const ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";
const LOOP: &str = "org.freedesktop.UDisks2.Loop";
const SWAPSPACE: &str = "org.freedesktop.UDisks2.Swapspace";
const JOB: &str = "org.freedesktop.UDisks2.Job";

pub(crate) type Props = HashMap<String, OwnedValue>;
type Interfaces = HashMap<OwnedInterfaceName, Props>;

#[derive(Debug, Default, PartialEq)]
pub struct Snapshot {
  /// udisksd answered with its manager
  pub running: bool,
  pub drives: Vec<Drive>,
  pub blocks: Vec<Block>,
  pub jobs: Vec<Job>,
  pub volumes: Vec<Volume>,
}

pub async fn snapshot(conn: &Connection) -> Result<Snapshot> {
  let objects = ObjectManagerProxy::new(conn, SERVICE, ROOT)
    .await?
    .get_managed_objects()
    .await?;
  Ok(read(&objects))
}

/// One drive read fresh, without waiting for its signals
pub async fn drive(conn: &Connection, path: OwnedObjectPath) -> Result<Option<Drive>> {
  let props = PropertiesProxy::builder(conn)
    .destination(SERVICE)?
    .path(path.clone())?
    .build()
    .await?;
  let mut interfaces = Interfaces::new();
  for name in [DRIVE, ATA, NVME_CONTROLLER] {
    let name = InterfaceName::from_static_str_unchecked(name);
    // the drive has only some of them
    if let Ok(all) = props.get_all(name.clone()).await {
      interfaces.insert(name.into(), all);
    }
  }
  Ok(read_drive(&path, &interfaces))
}

pub(crate) fn read(objects: &ManagedObjects) -> Snapshot {
  let mut paths: Vec<_> = objects.iter().collect();
  paths.sort_by_key(|(p, _)| p.as_str());
  fn each<T>(
    paths: &[(&OwnedObjectPath, &Interfaces)],
    read: fn(&OwnedObjectPath, &Interfaces) -> Option<T>,
  ) -> Vec<T> {
    paths.iter().filter_map(|(p, i)| read(p, i)).collect()
  }

  let drives = each(&paths, read_drive);
  let blocks = each(&paths, read_block);
  Snapshot {
    running: paths.iter().any(|(_, i)| iface(i, MANAGER).is_some()),
    jobs: each(&paths, read_job),
    volumes: volumes(&blocks, &drives),
    drives,
    blocks,
  }
}

fn iface<'a>(interfaces: &'a Interfaces, name: &str) -> Option<&'a Props> {
  interfaces
    .iter()
    .find(|(n, _)| n.as_str() == name)
    .map(|(_, props)| props)
}

pub(crate) fn get<T: TryFrom<OwnedValue>>(props: &Props, key: &str) -> Option<T> {
  T::try_from(props.get(key)?.try_clone().ok()?).ok()
}

fn string(props: &Props, key: &str) -> String {
  get(props, key).unwrap_or_default()
}

fn flag(props: &Props, key: &str) -> bool {
  get(props, key).unwrap_or(false)
}

pub(crate) fn num<T: TryFrom<OwnedValue> + Default>(props: &Props, key: &str) -> T {
  get(props, key).unwrap_or_default()
}

fn strings(props: &Props, key: &str) -> Vec<String> {
  get(props, key).unwrap_or_default()
}

/// An object reference, `/` meaning none
fn path(props: &Props, key: &str) -> Option<OwnedObjectPath> {
  get::<OwnedObjectPath>(props, key).filter(|p| p.as_str() != "/")
}

fn paths(props: &Props, key: &str) -> Vec<OwnedObjectPath> {
  get(props, key).unwrap_or_default()
}

fn nul_terminated(bytes: Vec<u8>) -> String {
  let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
  String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// `ay` byte strings, NUL terminated
fn bytes(props: &Props, key: &str) -> String {
  get::<Vec<u8>>(props, key)
    .map(nul_terminated)
    .unwrap_or_default()
}

/// `aay`
fn byte_strings(props: &Props, key: &str) -> Vec<String> {
  get::<Vec<Vec<u8>>>(props, key)
    .unwrap_or_default()
    .into_iter()
    .map(nul_terminated)
    .collect()
}

fn read_drive(path: &OwnedObjectPath, interfaces: &Interfaces) -> Option<Drive> {
  let props = iface(interfaces, DRIVE)?;
  let ata = iface(interfaces, ATA).map(|p| Ata {
    smart_supported: flag(p, "SmartSupported"),
    smart_enabled: flag(p, "SmartEnabled"),
    smart_failing: flag(p, "SmartFailing"),
    smart_updated: num(p, "SmartUpdated"),
    smart_temperature: num(p, "SmartTemperature"),
    smart_power_on_seconds: num(p, "SmartPowerOnSeconds"),
    smart_num_attributes_failing: num(p, "SmartNumAttributesFailing"),
    smart_num_bad_sectors: num(p, "SmartNumBadSectors"),
    smart_selftest_status: string(p, "SmartSelftestStatus"),
  });
  let nvme = iface(interfaces, NVME_CONTROLLER).map(|p| NvmeController {
    smart_updated: num(p, "SmartUpdated"),
    smart_critical_warning: strings(p, "SmartCriticalWarning"),
    smart_power_on_hours: num(p, "SmartPowerOnHours"),
    smart_temperature: num(p, "SmartTemperature"),
    smart_selftest_status: string(p, "SmartSelftestStatus"),
  });
  Some(Drive {
    path: path.clone(),
    id: string(props, "Id"),
    vendor: string(props, "Vendor"),
    model: string(props, "Model"),
    revision: string(props, "Revision"),
    serial: string(props, "Serial"),
    size: num(props, "Size"),
    media: string(props, "Media"),
    media_available: flag(props, "MediaAvailable"),
    removable: flag(props, "Removable"),
    ejectable: flag(props, "Ejectable"),
    can_power_off: flag(props, "CanPowerOff"),
    connection_bus: string(props, "ConnectionBus"),
    rotation_rate: num(props, "RotationRate"),
    sort_key: string(props, "SortKey"),
    ata,
    nvme,
  })
}

fn read_block(path: &OwnedObjectPath, interfaces: &Interfaces) -> Option<Block> {
  let props = iface(interfaces, BLOCK)?;
  Some(Block {
    path: path.clone(),
    device: bytes(props, "Device"),
    id: string(props, "Id"),
    id_label: string(props, "IdLabel"),
    id_uuid: string(props, "IdUUID"),
    id_type: string(props, "IdType"),
    size: num(props, "Size"),
    read_only: flag(props, "ReadOnly"),
    drive: self::path(props, "Drive"),
    hint_name: string(props, "HintName"),
    hint_icon_name: string(props, "HintIconName"),
    hint_system: flag(props, "HintSystem"),
    hint_ignore: flag(props, "HintIgnore"),
    crypto_backing_device: self::path(props, "CryptoBackingDevice"),
    filesystem: iface(interfaces, FILESYSTEM).map(|p| Filesystem {
      mount_points: byte_strings(p, "MountPoints"),
      size: num(p, "Size"),
    }),
    partition: iface(interfaces, PARTITION).map(|p| Partition {
      number: num(p, "Number"),
      offset: num(p, "Offset"),
      size: num(p, "Size"),
      kind: string(p, "Type"),
      name: string(p, "Name"),
      flags: num(p, "Flags"),
      table: self::path(p, "Table"),
    }),
    partition_table: iface(interfaces, PARTITION_TABLE).map(|p| PartitionTable {
      kind: string(p, "Type"),
    }),
    encrypted: iface(interfaces, ENCRYPTED).map(|p| Encrypted {
      cleartext_device: self::path(p, "CleartextDevice"),
      hint_encryption_type: string(p, "HintEncryptionType"),
    }),
    loop_device: iface(interfaces, LOOP).map(|p| Loop {
      backing_file: bytes(p, "BackingFile"),
      autoclear: flag(p, "Autoclear"),
    }),
    swapspace: iface(interfaces, SWAPSPACE).map(|p| Swapspace {
      active: flag(p, "Active"),
    }),
  })
}

fn read_job(path: &OwnedObjectPath, interfaces: &Interfaces) -> Option<Job> {
  let props = iface(interfaces, JOB)?;
  Some(Job {
    path: path.clone(),
    operation: string(props, "Operation"),
    progress: num(props, "Progress"),
    progress_valid: flag(props, "ProgressValid"),
    rate: num(props, "Rate"),
    start_time: num(props, "StartTime"),
    expected_end_time: num(props, "ExpectedEndTime"),
    objects: paths(props, "Objects"),
    cancelable: flag(props, "Cancelable"),
    bytes: num(props, "Bytes"),
  })
}

pub(crate) fn read_nvme_health(props: &Props) -> NvmeHealth {
  NvmeHealth {
    avail_spare: num(props, "avail_spare"),
    spare_thresh: num(props, "spare_thresh"),
    percent_used: num(props, "percent_used"),
    total_data_read: num(props, "total_data_read"),
    total_data_written: num(props, "total_data_written"),
    ctrl_busy_time: num(props, "ctrl_busy_time"),
    power_cycles: num(props, "power_cycles"),
    unsafe_shutdowns: num(props, "unsafe_shutdowns"),
    media_errors: num(props, "media_errors"),
    num_err_log_entries: num(props, "num_err_log_entries"),
    temp_sensors: get(props, "temp_sensors").unwrap_or_default(),
    wctemp: num(props, "wctemp"),
    cctemp: num(props, "cctemp"),
    warning_temp_time: num(props, "warning_temp_time"),
    critical_temp_time: num(props, "critical_temp_time"),
  }
}

/// The filesystems and encrypted containers that are not the system's, a cleartext listed
/// through its encrypted block
pub(crate) fn volumes(blocks: &[Block], drives: &[Drive]) -> Vec<Volume> {
  let block = |path: &OwnedObjectPath| blocks.iter().find(|b| &b.path == path);
  let mut volumes: Vec<Volume> = blocks
    .iter()
    .filter(|b| !b.hint_system && !b.hint_ignore && b.crypto_backing_device.is_none())
    .filter_map(|b| {
      let (shown, lock) = match &b.encrypted {
        Some(encrypted) => match encrypted.cleartext_device.as_ref().and_then(block) {
          Some(cleartext) => (cleartext, Some(Lock::Unlocked(cleartext.path.clone()))),
          None => (b, Some(Lock::Locked)),
        },
        None if b.filesystem.is_some() => (b, None),
        None => return None,
      };
      let drive = b
        .drive
        .as_ref()
        .and_then(|path| drives.iter().find(|d| &d.path == path));
      let label = [&shown.id_label, &b.id_label]
        .into_iter()
        .find(|l| !l.is_empty())
        .unwrap_or(&b.device)
        .clone();
      Some(Volume {
        block: shown.path.clone(),
        drive: drive.map(|d| d.path.clone()),
        device: b.device.clone(),
        label,
        model: drive.map(Drive::name).unwrap_or_default(),
        size: b.size,
        mount_points: shown
          .filesystem
          .as_ref()
          .map(|f| f.mount_points.clone())
          .unwrap_or_default(),
        removable: drive.is_some_and(Drive::is_removable),
        can_power_off: drive.is_some_and(|d| d.can_power_off),
        lock,
      })
    })
    .collect();
  sort_volumes(&mut volumes);
  volumes
}

#[cfg(test)]
mod tests {
  use zbus::zvariant::Value;

  use super::*;

  type TestProps = Vec<(&'static str, Value<'static>)>;

  fn objects(entries: Vec<(&str, Vec<(&str, TestProps)>)>) -> ManagedObjects {
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

  fn cstr(s: &str) -> Value<'static> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    Value::from(bytes)
  }

  fn mounts(points: &[&str]) -> Value<'static> {
    let points: Vec<Vec<u8>> = points
      .iter()
      .map(|p| {
        let mut bytes = p.as_bytes().to_vec();
        bytes.push(0);
        bytes
      })
      .collect();
    Value::from(points)
  }

  const B: &str = "/org/freedesktop/UDisks2/block_devices";
  const D: &str = "/org/freedesktop/UDisks2/drives";

  fn usb_stick() -> Vec<(&'static str, Vec<(&'static str, TestProps)>)> {
    vec![
      (
        "/org/freedesktop/UDisks2/Manager",
        vec![(MANAGER, vec![("Version", "2.11.2".into())])],
      ),
      (
        "/org/freedesktop/UDisks2/drives/usb",
        vec![(
          DRIVE,
          vec![
            ("Vendor", "SanDisk".into()),
            ("Model", "Cruzer".into()),
            ("Removable", true.into()),
            ("CanPowerOff", true.into()),
            ("ConnectionBus", "usb".into()),
            ("Media", "thumb".into()),
          ],
        )],
      ),
      (
        "/org/freedesktop/UDisks2/block_devices/sdb1",
        vec![
          (
            BLOCK,
            vec![
              ("Device", cstr("/dev/sdb1")),
              ("Drive", path(&format!("{D}/usb"))),
              ("IdLabel", "STICK".into()),
              ("IdType", "vfat".into()),
              ("IdUUID", "ABCD-1234".into()),
              ("HintIconName", "drive-removable-media".into()),
              ("Size", 8_000_000_000u64.into()),
              ("CryptoBackingDevice", path("/")),
            ],
          ),
          (
            FILESYSTEM,
            vec![("MountPoints", mounts(&["/run/media/me/STICK"]))],
          ),
          (
            PARTITION,
            vec![
              ("Number", 1u32.into()),
              ("Offset", 1_048_576u64.into()),
              ("Table", path(&format!("{B}/sdb"))),
            ],
          ),
        ],
      ),
      (
        "/org/freedesktop/UDisks2/block_devices/sdb",
        vec![
          (
            BLOCK,
            vec![
              ("Device", cstr("/dev/sdb")),
              ("Drive", path(&format!("{D}/usb"))),
            ],
          ),
          (PARTITION_TABLE, vec![("Type", "gpt".into())]),
        ],
      ),
    ]
  }

  #[test]
  fn reads_a_usb_stick() {
    let snapshot = read(&objects(usb_stick()));
    assert!(snapshot.running);

    let drive = &snapshot.drives[0];
    assert_eq!(drive.name(), "SanDisk Cruzer");
    assert_eq!(drive.media, "thumb");
    assert!(drive.is_removable() && drive.can_power_off);
    assert!(drive.ata.is_none() && drive.smart().is_none());

    // sorted by path
    let devices: Vec<_> = snapshot.blocks.iter().map(|b| b.device.as_str()).collect();
    assert_eq!(devices, ["/dev/sdb", "/dev/sdb1"]);
    assert_eq!(
      snapshot.blocks[0].partition_table.as_ref().unwrap().kind,
      "gpt"
    );
    let part = &snapshot.blocks[1];
    // `/` reads as none
    assert_eq!(part.crypto_backing_device, None);
    assert_eq!(
      (part.id_uuid.as_str(), part.hint_icon_name.as_str()),
      ("ABCD-1234", "drive-removable-media")
    );
    let partition = part.partition.as_ref().unwrap();
    assert_eq!((partition.number, partition.offset), (1, 1_048_576));
    assert_eq!(
      partition.table.as_ref().unwrap().as_str(),
      format!("{B}/sdb")
    );

    let volume = &snapshot.volumes[..];
    assert_eq!(volume.len(), 1);
    assert_eq!(volume[0].label, "STICK");
    assert_eq!(volume[0].model, "SanDisk Cruzer");
    assert_eq!(volume[0].mount_points, ["/run/media/me/STICK"]);
    assert!(volume[0].removable && volume[0].can_power_off && volume[0].lock.is_none());
  }

  fn encrypted(cleartext: Option<&str>) -> ManagedObjects {
    let mut entries = vec![(
      "/org/freedesktop/UDisks2/block_devices/sdc1",
      vec![
        (
          BLOCK,
          vec![
            ("Device", cstr("/dev/sdc1")),
            ("IdType", "crypto_LUKS".into()),
            ("Size", 100u64.into()),
          ],
        ),
        (
          ENCRYPTED,
          vec![
            ("HintEncryptionType", "luks2".into()),
            ("CleartextDevice", path(cleartext.unwrap_or("/"))),
          ],
        ),
      ],
    )];
    if let Some(cleartext) = cleartext {
      entries.push((
        cleartext,
        vec![
          (
            BLOCK,
            vec![
              ("Device", cstr("/dev/dm-0")),
              ("IdLabel", "SECRET".into()),
              ("CryptoBackingDevice", path(&format!("{B}/sdc1"))),
            ],
          ),
          (FILESYSTEM, vec![("MountPoints", mounts(&["/mnt/secret"]))]),
        ],
      ));
    }
    objects(entries)
  }

  #[test]
  fn encrypted_volumes() {
    let locked = read(&encrypted(None));
    // no manager: udisksd is not up
    assert!(!locked.running);
    assert_eq!(locked.volumes.len(), 1);
    let volume = &locked.volumes[0];
    assert!(volume.is_locked() && !volume.is_mounted());
    assert_eq!(volume.block.as_str(), format!("{B}/sdc1"));
    // no label anywhere: the device
    assert_eq!(volume.label, "/dev/sdc1");
    assert_eq!(
      volume.encrypted_block(&locked.blocks).unwrap().as_str(),
      format!("{B}/sdc1")
    );
    assert_eq!(
      locked.blocks[0]
        .encrypted
        .as_ref()
        .unwrap()
        .hint_encryption_type,
      "luks2"
    );

    let cleartext = format!("{B}/dm_2d0");
    let unlocked = read(&encrypted(Some(&cleartext)));
    // the cleartext is listed through its encrypted block only
    assert_eq!(unlocked.volumes.len(), 1);
    let volume = &unlocked.volumes[0];
    assert_eq!(volume.block.as_str(), cleartext);
    assert_eq!(volume.device, "/dev/sdc1");
    assert_eq!(volume.label, "SECRET");
    assert_eq!(volume.mount_points, ["/mnt/secret"]);
    assert!(!volume.is_locked());
    assert_eq!(
      volume.encrypted_block(&unlocked.blocks).unwrap().as_str(),
      format!("{B}/sdc1")
    );
  }

  #[test]
  fn system_hidden_and_bare_blocks_are_no_volumes() {
    let snapshot = read(&objects(vec![
      (
        "/b/root",
        vec![
          (
            BLOCK,
            vec![
              ("Device", cstr("/dev/nvme0n1p2")),
              ("HintSystem", true.into()),
            ],
          ),
          (FILESYSTEM, vec![]),
        ],
      ),
      (
        "/b/hidden",
        vec![
          (
            BLOCK,
            vec![("Device", cstr("/dev/sda1")), ("HintIgnore", true.into())],
          ),
          (FILESYSTEM, vec![]),
        ],
      ),
      (
        "/b/swap",
        vec![
          (BLOCK, vec![("Device", cstr("/dev/sda2"))]),
          (SWAPSPACE, vec![("Active", true.into())]),
        ],
      ),
      // a loop device's filesystem is listed, without a drive
      (
        "/b/loop0",
        vec![
          (
            BLOCK,
            vec![("Device", cstr("/dev/loop0")), ("Drive", path("/"))],
          ),
          (FILESYSTEM, vec![]),
          (
            LOOP,
            vec![
              ("BackingFile", cstr("/tmp/disk.img")),
              ("Autoclear", true.into()),
            ],
          ),
        ],
      ),
    ]));
    assert_eq!(snapshot.blocks.len(), 4);
    let swap = snapshot
      .blocks
      .iter()
      .find(|b| b.swapspace.is_some())
      .unwrap();
    assert!(swap.swapspace.as_ref().unwrap().active);
    let devices: Vec<_> = snapshot.volumes.iter().map(|v| v.device.as_str()).collect();
    assert_eq!(devices, ["/dev/loop0"]);
    let volume = &snapshot.volumes[0];
    assert!(volume.drive.is_none() && !volume.removable && volume.model.is_empty());
    let looped = snapshot
      .blocks
      .iter()
      .find(|b| b.loop_device.is_some())
      .unwrap()
      .loop_device
      .as_ref()
      .unwrap();
    assert_eq!(looped.backing_file, "/tmp/disk.img");
    assert!(looped.autoclear);
  }

  #[test]
  fn smart_and_jobs() {
    let snapshot = read(&objects(vec![
      (
        "/d/ssd",
        vec![
          (DRIVE, vec![("Model", "SSD".into())]),
          (
            ATA,
            vec![
              ("SmartSupported", true.into()),
              ("SmartTemperature", 300.0f64.into()),
              ("SmartSelftestStatus", "success".into()),
              ("SmartNumBadSectors", 3i64.into()),
            ],
          ),
        ],
      ),
      (
        "/d/nvme",
        vec![
          (DRIVE, vec![]),
          (
            NVME_CONTROLLER,
            vec![
              ("SmartTemperature", 320u16.into()),
              ("SmartPowerOnHours", 1234u64.into()),
              ("SmartCriticalWarning", Vec::<String>::new().into()),
            ],
          ),
        ],
      ),
      (
        "/j/1",
        vec![(
          JOB,
          vec![
            ("Operation", "format-mkfs".into()),
            ("Progress", 0.5f64.into()),
            ("ProgressValid", true.into()),
            ("Rate", 1000u64.into()),
            ("StartTime", 42u64.into()),
            ("Objects", Value::from(vec![path("/b/sdb1")])),
            ("Cancelable", true.into()),
          ],
        )],
      ),
    ]));
    let ssd = snapshot.drives.iter().find(|d| d.model == "SSD").unwrap();
    assert_eq!(ssd.ata.as_ref().unwrap().smart_num_bad_sectors, 3);
    let smart = ssd.smart().unwrap();
    assert!((smart.temperature_celsius.unwrap() - 26.85).abs() < 1e-9);
    assert_eq!(smart.selftest_status, "success");
    let nvme = snapshot.drives.iter().find(|d| d.nvme.is_some()).unwrap();
    let smart = nvme.smart().unwrap();
    assert!(!smart.failing);
    assert_eq!(smart.power_on_hours, Some(1234));

    let job = &snapshot.jobs[0];
    assert_eq!(job.operation, "format-mkfs");
    assert!(job.progress_valid && job.cancelable && job.progress == 0.5);
    assert_eq!((job.rate, job.start_time), (1000, 42));
    assert_eq!(job.objects[0].as_str(), "/b/sdb1");
  }

  #[test]
  fn nvme_health_log() {
    let props: Props = [
      ("avail_spare", Value::from(100u8)),
      ("percent_used", Value::from(3u8)),
      ("total_data_written", Value::from(55_071_208_448_000u64)),
      ("unsafe_shutdowns", Value::from(7u64)),
      ("temp_sensors", Value::from(vec![310u16, 315])),
      // mistyped: default
      ("media_errors", Value::from("none")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
    .collect();
    let health = read_nvme_health(&props);
    assert_eq!((health.avail_spare, health.percent_used), (100, 3));
    assert_eq!(health.total_data_written, 55_071_208_448_000);
    assert_eq!(health.unsafe_shutdowns, 7);
    assert_eq!(health.temp_sensors, [310, 315]);
    assert_eq!(health.media_errors, 0);
  }

  #[test]
  fn mistyped_props_read_as_defaults() {
    let snapshot = read(&objects(vec![(
      "/b/x",
      vec![
        (
          BLOCK,
          vec![("Device", "not bytes".into()), ("Size", "big".into())],
        ),
        (FILESYSTEM, vec![("MountPoints", "nope".into())]),
      ],
    )]));
    let block = &snapshot.blocks[0];
    assert_eq!((block.device.as_str(), block.size), ("", 0));
    assert!(block.filesystem.as_ref().unwrap().mount_points.is_empty());
    assert!(read(&ManagedObjects::new()) == Snapshot::default());
  }
}
