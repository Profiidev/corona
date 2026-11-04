//! The panel's view of udisks' state: drives with their partitions, health, usage, free space

use crate::{Block, Drive, Job};
use rust_i18n::t;
use std::collections::{HashMap, HashSet};
use zbus::zvariant::OwnedObjectPath;

/// Partition colors by position, as in the design
const HUES: [f32; 7] = [200., 250., 30., 320., 215., 85., 160.];

#[derive(Clone, Debug, PartialEq)]
pub struct DriveView {
  pub drive: Drive,
  /// The whole-disk block
  pub disk: Option<Block>,
  pub parts: Vec<PartView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartView {
  pub block: Block,
  /// Set while an encrypted partition is unlocked
  pub cleartext: Option<Block>,
  pub hue: f32,
}

/// Where users mount their own drives; anything mounted elsewhere the running system uses
const USER_MOUNTS: [&str; 4] = ["/run/media/", "/media/", "/mnt/", "/run/user/"];

/// Mounted where the system needs it, like `/`, `/boot` or `/nix`, or swap in use
pub fn system_block(block: &Block) -> bool {
  let mounted = block.filesystem.as_ref().is_some_and(|f| {
    f.mount_points
      .iter()
      .any(|m| !USER_MOUNTS.iter().any(|u| m.starts_with(u)))
  });
  mounted || block.swapspace.as_ref().is_some_and(|s| s.active)
}

impl DriveView {
  /// One of its partitions the running system uses
  pub fn is_system(&self) -> bool {
    self.parts.iter().any(PartView::is_system)
  }
}

impl PartView {
  /// The running system uses it, or what is unlocked in it: no changing it
  pub fn is_system(&self) -> bool {
    system_block(&self.block) || self.cleartext.as_ref().is_some_and(system_block)
  }

  /// Mounted, unlocked or active, by what it is
  pub fn is_on(&self) -> bool {
    match (&self.block.encrypted, &self.block.swapspace) {
      (Some(_), _) => self.cleartext.is_some(),
      (_, Some(swap)) => swap.active,
      _ => self.block.is_mounted(),
    }
  }
}

pub fn hue(index: usize) -> f32 {
  HUES[index % HUES.len()]
}

/// Every drive with its whole-disk block and partitions, by udisks' sort key
pub fn drives(drives: &[Drive], blocks: &[Block]) -> Vec<DriveView> {
  let cleartext = |of: &Block| {
    blocks
      .iter()
      .find(|b| b.crypto_backing_device.as_ref() == Some(&of.path))
      .cloned()
  };
  let mut views: Vec<DriveView> = drives
    .iter()
    .map(|drive| {
      let disk = blocks
        .iter()
        .find(|b| {
          b.drive.as_ref() == Some(&drive.path)
            && b.partition.is_none()
            && b.crypto_backing_device.is_none()
        })
        .cloned();
      let mut parts: Vec<&Block> = blocks
        .iter()
        .filter(|b| {
          let table = b.partition.as_ref().and_then(|p| p.table.as_ref());
          disk.as_ref().is_some_and(|d| table == Some(&d.path))
        })
        .collect();
      parts.sort_by_key(|b| b.partition.as_ref().map(|p| p.number));
      // a filesystem on the whole disk, no table
      if parts.is_empty()
        && let Some(disk) = disk
          .as_ref()
          .filter(|d| d.filesystem.is_some() || d.encrypted.is_some() || d.swapspace.is_some())
      {
        parts.push(disk);
      }
      let parts = parts
        .into_iter()
        .enumerate()
        .map(|(i, b)| PartView {
          block: b.clone(),
          cleartext: cleartext(b),
          hue: hue(i),
        })
        .collect();
      DriveView {
        drive: drive.clone(),
        disk,
        parts,
      }
    })
    .collect();
  views.sort_by(|a, b| a.drive.sort_key.cmp(&b.drive.sort_key));
  views
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
  Ok,
  Warn,
  Err,
  Unknown,
}

pub fn health(drive: &Drive) -> Health {
  let Some(smart) = drive.smart() else {
    return Health::Unknown;
  };
  if smart.failing {
    return Health::Err;
  }
  match &drive.ata {
    Some(ata) if ata.smart_num_attributes_failing > 0 || ata.smart_num_bad_sectors > 0 => {
      Health::Warn
    }
    _ => Health::Ok,
  }
}

/// Mount point usage as last read off the UI thread, statvfs can hang on a dead device
#[derive(Default)]
pub struct UsageCache {
  /// The mount points of the current volumes
  wanted: HashSet<String>,
  /// Mount points with a read in flight, never read twice at once
  pending: HashSet<String>,
  /// A read's result, None when the stat failed; kept while a fresh read runs
  values: HashMap<String, Option<(u64, u64)>>,
}

impl UsageCache {
  /// Used and total bytes, None while unread or unreadable
  pub fn get(&self, mount_point: &str) -> Option<(u64, u64)> {
    self.values.get(mount_point).copied().flatten()
  }

  /// Follows the volumes: drops unmounted ones, returns the mount points to read now
  pub fn sync(&mut self, mounts: HashSet<String>) -> Vec<String> {
    self.values.retain(|m, _| mounts.contains(m));
    self.wanted = mounts;
    let todo: Vec<String> = self
      .wanted
      .iter()
      .filter(|m| !self.pending.contains(*m))
      .cloned()
      .collect();
    self.pending.extend(todo.iter().cloned());
    todo
  }

  /// Stores a read's result unless the mount point went away meanwhile
  pub fn finish(&mut self, mount_point: String, value: Option<(u64, u64)>) {
    self.pending.remove(&mount_point);
    if self.wanted.contains(&mount_point) {
      self.values.insert(mount_point, value);
    }
  }
}

/// The cached usage of a mount point
pub fn cached_usage(mount_point: &str, cx: &gpui_kit::App) -> Option<(u64, u64)> {
  cx.try_global::<UsageCache>()?.get(mount_point)
}

/// Used and total bytes of a mounted filesystem, blocks on a hung one: never on the UI thread
pub fn usage(mount_point: &str) -> Option<(u64, u64)> {
  let stat = rustix::fs::statvfs(mount_point).ok()?;
  used_total(stat.f_blocks, stat.f_bfree, stat.f_frsize)
}

/// Used and total bytes from statvfs counts, free may exceed total on odd filesystems
fn used_total(blocks: u64, free_blocks: u64, frsize: u64) -> Option<(u64, u64)> {
  let total = blocks.saturating_mul(frsize);
  let free = free_blocks.saturating_mul(frsize);
  (total > 0).then(|| (total.saturating_sub(free), total))
}

/// Percent used
pub fn percent(used: u64, total: u64) -> f32 {
  match total {
    0 => 0.,
    total => used as f32 * 100. / total as f32,
  }
}

const MIB: u64 = 1 << 20;

/// Gaps of at least 1 MiB between `parts` (offset, size), 1 MiB aligned, the first and last MiB
/// kept for the table
pub fn free_spaces(disk_size: u64, parts: &[(u64, u64)]) -> Vec<(u64, u64)> {
  let mut parts = parts.to_vec();
  parts.sort();
  let end = disk_size.saturating_sub(MIB) / MIB * MIB;
  let mut gaps = Vec::new();
  let mut at = MIB;
  for (offset, size) in parts.into_iter().chain([(end, 0)]) {
    let start = at.div_ceil(MIB) * MIB;
    let stop = offset.min(end) / MIB * MIB;
    if stop >= start + MIB {
      gaps.push((start, stop - start));
    }
    at = at.max(offset + size);
  }
  gaps
}

/// The udisks job operations with a name of their own, the rest is spelled out
pub const OPERATIONS: [&str; 25] = [
  "ata-smart-selftest",
  "ata-smart-update",
  "cleanup",
  "drive-eject",
  "drive-power-off",
  "encrypted-lock",
  "encrypted-modify",
  "encrypted-resize",
  "encrypted-unlock",
  "filesystem-check",
  "filesystem-modify",
  "filesystem-mount",
  "filesystem-repair",
  "filesystem-resize",
  "filesystem-unmount",
  "format-erase",
  "format-mkfs",
  "loop-modify",
  "loop-setup",
  "partition-create",
  "partition-delete",
  "partition-modify",
  "swapspace-modify",
  "swapspace-start",
  "swapspace-stop",
];

/// What a job is doing, named, with the label of what it works on
pub fn job_title(operation: &str, objects: &[OwnedObjectPath], blocks: &[Block]) -> String {
  let key = format!("app.disks.operation.{}", operation.replace('-', "_"));
  let name = match crate::_rust_i18n_try_translate(&rust_i18n::locale(), &key) {
    Some(name) if OPERATIONS.contains(&operation) => name.to_string(),
    _ if operation.is_empty() => t!("app.disks.operation.unknown").to_string(),
    _ => {
      let mut name = operation.replace('-', " ");
      if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
      }
      name
    }
  };
  let target = objects
    .iter()
    .find_map(|o| blocks.iter().find(|b| &b.path == o))
    .map(|b| b.name().to_string());
  match target {
    Some(target) => format!("{name} · {target}"),
    None => name,
  }
}

/// Seconds left, from the job's expected end
pub fn remaining(job: &Job, now_micros: u64) -> Option<u64> {
  (job.expected_end_time > now_micros).then(|| (job.expected_end_time - now_micros) / 1_000_000)
}

#[cfg(test)]
pub(crate) mod tests {
  use crate::{Ata, Encrypted, Filesystem, Partition, PartitionTable, Swapspace};

  use super::*;

  #[test]
  fn usage_cache_reads_once_and_drops_unmounted() {
    let set = |m: &[&str]| m.iter().map(|m| m.to_string()).collect::<HashSet<_>>();
    let mut cache = UsageCache::default();
    assert_eq!(cache.sync(set(&["/a"])), vec!["/a"]);
    assert_eq!(cache.get("/a"), None);
    // a read in flight is not stacked, a new mount point starts its own
    let mut todo = cache.sync(set(&["/a", "/b"]));
    todo.sort();
    assert_eq!(todo, vec!["/b"]);
    cache.finish("/a".into(), Some((1, 2)));
    assert_eq!(cache.get("/a"), Some((1, 2)));
    // the old value shows while the refresh runs
    assert_eq!(cache.sync(set(&["/a"])), vec!["/a"]);
    assert_eq!(cache.get("/a"), Some((1, 2)));
    // unmounted: entry dropped, a late result discarded
    cache.sync(set(&[]));
    assert_eq!(cache.get("/a"), None);
    cache.finish("/b".into(), Some((3, 4)));
    assert_eq!(cache.get("/b"), None);
  }

  #[test]
  fn used_total_survives_free_above_total() {
    assert_eq!(used_total(10, 20, 4096), Some((0, 40960)));
    assert_eq!(used_total(10, 4, 4096), Some((6 * 4096, 40960)));
    assert_eq!(used_total(0, 0, 4096), None);
    assert_eq!(used_total(u64::MAX, 0, 2), Some((u64::MAX, u64::MAX)));
  }

  pub(crate) fn path(p: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(p).unwrap()
  }

  pub(crate) fn drive(id: &str, removable: bool) -> Drive {
    Drive {
      path: path(&format!("/org/freedesktop/UDisks2/drives/{id}")),
      id: id.into(),
      vendor: "Vendor".into(),
      model: id.into(),
      revision: "1.0".into(),
      serial: format!("SN-{id}"),
      size: 32_000_000_000,
      media: String::new(),
      media_available: true,
      removable,
      ejectable: removable,
      can_power_off: removable,
      connection_bus: if removable {
        "usb".into()
      } else {
        String::new()
      },
      rotation_rate: 0,
      sort_key: id.into(),
      ata: None,
      nvme: None,
    }
  }

  pub(crate) fn block(name: &str, drive: Option<&Drive>) -> Block {
    Block {
      path: path(&format!("/org/freedesktop/UDisks2/block_devices/{name}")),
      device: format!("/dev/{name}"),
      id: String::new(),
      id_label: String::new(),
      id_uuid: String::new(),
      id_type: String::new(),
      size: 1_000_000_000,
      read_only: false,
      drive: drive.map(|d| d.path.clone()),
      hint_name: String::new(),
      hint_icon_name: String::new(),
      hint_system: false,
      hint_ignore: false,
      crypto_backing_device: None,
      filesystem: None,
      partition: None,
      partition_table: None,
      encrypted: None,
      loop_device: None,
      swapspace: None,
    }
  }

  pub(crate) fn part(name: &str, drive: &Drive, disk: &Block, number: u32) -> Block {
    let mut b = block(name, Some(drive));
    b.partition = Some(Partition {
      number,
      offset: number as u64 * 1_000_000_000,
      size: 1_000_000_000,
      kind: String::new(),
      name: String::new(),
      flags: 0,
      table: Some(disk.path.clone()),
    });
    b
  }

  /// A usb stick: a mounted ext4, a locked LUKS and a swap partition
  pub(crate) fn stick() -> (Vec<Drive>, Vec<Block>) {
    let d = drive("usb", true);
    let mut disk = block("sdb", Some(&d));
    disk.partition_table = Some(PartitionTable { kind: "gpt".into() });
    let mut data = part("sdb1", &d, &disk, 1);
    data.id_type = "ext4".into();
    data.id_label = "STICK".into();
    data.filesystem = Some(Filesystem {
      mount_points: vec!["/run/media/me/STICK".into()],
      size: 1_000_000_000,
    });
    let mut secret = part("sdb2", &d, &disk, 2);
    secret.id_type = "crypto_LUKS".into();
    secret.encrypted = Some(Encrypted::default());
    let mut swap = part("sdb3", &d, &disk, 3);
    swap.id_type = "swap".into();
    swap.swapspace = Some(Swapspace { active: false });
    (vec![d], vec![disk, data, secret, swap])
  }

  #[test]
  fn drives_with_their_partitions() {
    let (drives, mut blocks) = stick();
    let mut internal = drive("nvme", false);
    internal.sort_key = "00".into();
    blocks.push(block("nvme0n1", Some(&internal)));
    let views = super::drives(&[drives[0].clone(), internal], &blocks);
    assert_eq!(views[0].drive.id, "nvme");
    assert!(views[0].parts.is_empty());
    let usb = &views[1];
    assert_eq!(usb.disk.as_ref().unwrap().device, "/dev/sdb");
    let names: Vec<_> = usb.parts.iter().map(|p| p.block.device.as_str()).collect();
    assert_eq!(names, ["/dev/sdb1", "/dev/sdb2", "/dev/sdb3"]);
    assert_eq!(usb.parts[1].hue, HUES[1]);
    let on: Vec<_> = usb.parts.iter().map(PartView::is_on).collect();
    assert_eq!(on, [true, false, false]);
  }

  #[test]
  fn unlocked_cleartext_and_bare_filesystems() {
    let (drives, mut blocks) = stick();
    let mut clear = block("dm_2d0", None);
    clear.device = "/dev/dm-0".into();
    clear.crypto_backing_device = Some(blocks[2].path.clone());
    clear.filesystem = Some(Filesystem::default());
    blocks.push(clear);
    let views = super::drives(&drives, &blocks);
    let luks = &views[0].parts[1];
    assert!(luks.is_on());
    assert_eq!(luks.cleartext.as_ref().unwrap().device, "/dev/dm-0");

    // a superfloppy: the whole disk is the filesystem
    let d = drive("floppy", true);
    let mut disk = block("sdc", Some(&d));
    disk.filesystem = Some(Filesystem::default());
    let views = super::drives(&[d], &[disk]);
    assert_eq!(views[0].parts[0].block.device, "/dev/sdc");
  }

  #[test]
  fn the_running_systems_partitions() {
    let (drives, mut blocks) = stick();
    let views = super::drives(&drives, &blocks);
    assert!(!views[0].is_system());
    blocks[1].filesystem.as_mut().unwrap().mount_points = vec!["/".into()];
    let views = super::drives(&drives, &blocks);
    assert!(views[0].parts[0].is_system() && views[0].is_system());
    assert!(!views[0].parts[1].is_system() && !views[0].parts[2].is_system());
    for (mount, system) in [
      ("/run/media/me/STICK", false),
      ("/mnt/media", false),
      ("/media/usb", false),
      ("/boot", true),
      ("/nix/store", true),
      ("/home", true),
    ] {
      blocks[1].filesystem.as_mut().unwrap().mount_points = vec![mount.into()];
      assert_eq!(system_block(&blocks[1]), system, "{mount}");
    }
    // swap in use, and a LUKS whose cleartext is `/home`
    blocks[3].swapspace.as_mut().unwrap().active = true;
    assert!(system_block(&blocks[3]));
    let mut clear = block("dm_2d1", None);
    clear.crypto_backing_device = Some(blocks[2].path.clone());
    clear.filesystem = Some(Filesystem {
      mount_points: vec!["/home".into()],
      size: 0,
    });
    blocks.push(clear);
    let views = super::drives(&drives, &blocks);
    assert!(views[0].parts[1].is_system());
  }

  #[test]
  fn health_from_smart() {
    let mut d = drive("hdd", false);
    assert_eq!(health(&d), Health::Unknown);
    d.ata = Some(Ata {
      smart_supported: true,
      ..Default::default()
    });
    assert_eq!(health(&d), Health::Ok);
    d.ata.as_mut().unwrap().smart_num_bad_sectors = 8;
    assert_eq!(health(&d), Health::Warn);
    d.ata.as_mut().unwrap().smart_failing = true;
    assert_eq!(health(&d), Health::Err);
  }

  #[test]
  fn free_space_between_partitions() {
    const GB: u64 = 1 << 30;
    // empty disk: all but the first and last MiB
    assert_eq!(free_spaces(GB, &[]), [(MIB, GB - 2 * MIB)]);
    let parts = [(MIB, 100 * MIB), (300 * MIB, 100 * MIB)];
    assert_eq!(
      free_spaces(GB, &parts),
      [(101 * MIB, 199 * MIB), (400 * MIB, GB - MIB - 400 * MIB)]
    );
    // full disk, unaligned tiny gap skipped
    assert!(free_spaces(GB, &[(MIB, GB - 2 * MIB - 1000)]).is_empty());
    assert_eq!(percent(25, 100), 25.);
    assert_eq!(percent(1, 0), 0.);
  }

  #[test]
  fn job_titles_and_time_left() {
    let (_, blocks) = stick();
    assert_eq!(
      job_title("filesystem-check", &[blocks[1].path.clone()], &blocks),
      "Filesystem check · STICK"
    );
    assert_eq!(job_title("frobnicate-it", &[], &blocks), "Frobnicate it");
    let job = Job {
      path: path("/j"),
      operation: String::new(),
      progress: 0.,
      progress_valid: false,
      rate: 0,
      start_time: 0,
      expected_end_time: 90_000_000,
      objects: vec![],
      cancelable: true,
      bytes: 0,
    };
    assert_eq!(remaining(&job, 30_000_000), Some(60));
    assert_eq!(remaining(&job, 95_000_000), None);
  }
}
