use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
  pub path: OwnedObjectPath,
  /// Stable across reboots, like `Samsung_SSD_990_PRO_S7DS…`
  pub id: String,
  pub vendor: String,
  pub model: String,
  pub revision: String,
  pub serial: String,
  pub size: u64,
  /// Like `thumb` or `optical_cd`, empty when unknown
  pub media: String,
  pub media_available: bool,
  pub removable: bool,
  pub ejectable: bool,
  /// Whether `PowerOff` works, else ejecting falls back to `Eject`
  pub can_power_off: bool,
  /// `usb`, `sdio`, `ieee1394` or empty
  pub connection_bus: String,
  /// rpm, 0 for none, -1 unknown
  pub rotation_rate: i32,
  pub sort_key: String,
  pub ata: Option<Ata>,
  pub nvme: Option<NvmeController>,
}

impl Drive {
  /// Vendor and model
  pub fn name(&self) -> String {
    format!("{} {}", self.vendor, self.model).trim().to_string()
  }

  /// Comes out of the machine
  pub fn is_removable(&self) -> bool {
    self.removable || self.ejectable
  }

  pub fn smart(&self) -> Option<Smart> {
    if let Some(ata) = self.ata.as_ref().filter(|a| a.smart_supported) {
      return Some(Smart {
        enabled: ata.smart_enabled,
        updated: ata.smart_updated,
        failing: ata.smart_failing,
        temperature_celsius: (ata.smart_temperature > 0.).then_some(ata.smart_temperature - 273.15),
        power_on_hours: Some(ata.smart_power_on_seconds / 3600).filter(|h| *h > 0),
        selftest_status: ata.smart_selftest_status.clone(),
      });
    }
    let nvme = self.nvme.as_ref()?;
    Some(Smart {
      enabled: true,
      updated: nvme.smart_updated,
      failing: !nvme.smart_critical_warning.is_empty(),
      temperature_celsius: (nvme.smart_temperature > 0)
        .then_some(nvme.smart_temperature as f64 - 273.15),
      power_on_hours: Some(nvme.smart_power_on_hours).filter(|h| *h > 0),
      selftest_status: nvme.smart_selftest_status.clone(),
    })
  }
}

/// SMART of either an ATA or an NVMe drive
#[derive(Clone, Debug, PartialEq)]
pub struct Smart {
  pub enabled: bool,
  /// Unix seconds, 0 never
  pub updated: u64,
  pub failing: bool,
  pub temperature_celsius: Option<f64>,
  pub power_on_hours: Option<u64>,
  /// The last self-test's result, like `success`, `inprogress` or `aborted`. udisks keeps no
  /// history of earlier ones
  pub selftest_status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ata {
  pub smart_supported: bool,
  pub smart_enabled: bool,
  pub smart_failing: bool,
  /// Unix seconds, 0 never
  pub smart_updated: u64,
  /// Kelvin, 0 unknown
  pub smart_temperature: f64,
  pub smart_power_on_seconds: u64,
  pub smart_num_attributes_failing: i32,
  /// -1 unknown
  pub smart_num_bad_sectors: i64,
  pub smart_selftest_status: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NvmeController {
  /// Unix seconds, 0 never
  pub smart_updated: u64,
  /// Empty when healthy, like `temperature` or `spare`
  pub smart_critical_warning: Vec<String>,
  pub smart_power_on_hours: u64,
  /// Kelvin, 0 unknown
  pub smart_temperature: u16,
  pub smart_selftest_status: String,
}

/// One SMART attribute of an ATA drive, from `SmartGetAttributes`
#[derive(Clone, Debug, PartialEq)]
pub struct AtaAttribute {
  pub id: u8,
  pub name: String,
  pub flags: u16,
  /// Normalized, -1 unknown
  pub value: i32,
  pub worst: i32,
  pub threshold: i32,
  pub pretty: i64,
  /// 0 unknown, 1 dimensionless, 2 ms, 3 sectors, 4 mK
  pub pretty_unit: i32,
}

/// An NVMe controller's SMART/Health log page, from `SmartGetAttributes`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NvmeHealth {
  /// Percent of spare capacity left
  pub avail_spare: u8,
  /// Below this the controller warns
  pub spare_thresh: u8,
  /// Of the rated endurance, can pass 100
  pub percent_used: u8,
  /// Bytes, udisks converts the log's 512 000 byte units
  pub total_data_read: u64,
  pub total_data_written: u64,
  /// Minutes
  pub ctrl_busy_time: u64,
  pub power_cycles: u64,
  pub unsafe_shutdowns: u64,
  pub media_errors: u64,
  pub num_err_log_entries: u64,
  /// Kelvin per sensor
  pub temp_sensors: Vec<u16>,
  /// Warning and critical temperatures, Kelvin
  pub wctemp: u16,
  pub cctemp: u16,
  /// Minutes spent above each
  pub warning_temp_time: u32,
  pub critical_temp_time: u32,
}

/// What `SmartGetAttributes` answers, by drive kind
#[derive(Clone, Debug, PartialEq)]
pub enum SmartAttributes {
  Ata(Vec<AtaAttribute>),
  Nvme(NvmeHealth),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
  pub path: OwnedObjectPath,
  /// Like `/dev/sda1`
  pub device: String,
  pub id: String,
  pub id_label: String,
  pub id_uuid: String,
  /// Like `ext4`, `crypto_LUKS` or `swap`
  pub id_type: String,
  pub size: u64,
  pub read_only: bool,
  pub drive: Option<OwnedObjectPath>,
  pub hint_name: String,
  pub hint_icon_name: String,
  /// Not shown to users, like the root filesystem
  pub hint_system: bool,
  pub hint_ignore: bool,
  /// The encrypted device this one is the cleartext of
  pub crypto_backing_device: Option<OwnedObjectPath>,
  pub filesystem: Option<Filesystem>,
  pub partition: Option<Partition>,
  pub partition_table: Option<PartitionTable>,
  pub encrypted: Option<Encrypted>,
  pub loop_device: Option<Loop>,
  pub swapspace: Option<Swapspace>,
}

impl Block {
  /// Its label, its device otherwise
  pub fn name(&self) -> &str {
    match self.id_label.is_empty() {
      true => &self.device,
      false => &self.id_label,
    }
  }

  pub fn is_mounted(&self) -> bool {
    self
      .filesystem
      .as_ref()
      .is_some_and(|f| !f.mount_points.is_empty())
  }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filesystem {
  pub mount_points: Vec<String>,
  /// 0 when unknown
  pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Partition {
  pub number: u32,
  pub offset: u64,
  pub size: u64,
  /// A GUID on gpt, like `0x83` on dos
  pub kind: String,
  pub name: String,
  pub flags: u64,
  pub table: Option<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartitionTable {
  /// `gpt` or `dos`
  pub kind: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Encrypted {
  /// Set while unlocked
  pub cleartext_device: Option<OwnedObjectPath>,
  /// `luks1`, `luks2`, `bitlk`, …
  pub hint_encryption_type: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loop {
  pub backing_file: String,
  pub autoclear: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Swapspace {
  pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
  pub path: OwnedObjectPath,
  /// Like `filesystem-mount`, `format-mkfs` or `ata-smart-selftest`
  pub operation: String,
  /// 0 to 1, only with `progress_valid`
  pub progress: f64,
  pub progress_valid: bool,
  /// Bytes per second, 0 unknown
  pub rate: u64,
  /// Unix microseconds
  pub start_time: u64,
  /// Unix microseconds, 0 unknown
  pub expected_end_time: u64,
  pub objects: Vec<OwnedObjectPath>,
  pub cancelable: bool,
  /// What the job works through, 0 unknown
  pub bytes: u64,
}

/// A job's `Completed` signal, with what the job was as last seen
#[derive(Clone, Debug, PartialEq)]
pub struct JobCompleted {
  pub job: OwnedObjectPath,
  /// Empty when the job ended before it was seen
  pub operation: String,
  pub objects: Vec<OwnedObjectPath>,
  pub success: bool,
  /// Why it failed, empty on success
  pub message: String,
}

/// What an encrypted volume is at
#[derive(Clone, Debug, PartialEq)]
pub enum Lock {
  Locked,
  /// The cleartext block
  Unlocked(OwnedObjectPath),
}

/// A filesystem or encrypted container a user would mount: not a system one, not hidden
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
  /// The block to mount, the encrypted one for a locked volume
  pub block: OwnedObjectPath,
  /// Loop devices have none
  pub drive: Option<OwnedObjectPath>,
  /// Like `/dev/sdb1`, the encrypted one's for a cleartext
  pub device: String,
  pub label: String,
  /// The drive's vendor and model
  pub model: String,
  pub size: u64,
  pub mount_points: Vec<String>,
  pub removable: bool,
  pub can_power_off: bool,
  /// Set for an encrypted volume. Unlocked, `block` and `mount_points` are its cleartext's
  pub lock: Option<Lock>,
}

impl Volume {
  pub fn is_mounted(&self) -> bool {
    !self.mount_points.is_empty()
  }

  pub fn is_locked(&self) -> bool {
    self.lock == Some(Lock::Locked)
  }

  /// The encrypted block, for `lock`
  pub fn encrypted_block(&self, blocks: &[Block]) -> Option<OwnedObjectPath> {
    match &self.lock {
      Some(Lock::Locked) => Some(self.block.clone()),
      Some(Lock::Unlocked(_)) => blocks
        .iter()
        .find(|b| b.path == self.block)?
        .crypto_backing_device
        .clone(),
      None => None,
    }
  }
}

pub(crate) fn sort_volumes(volumes: &mut [Volume]) {
  volumes.sort_by(|a, b| a.device.cmp(&b.device));
}

#[cfg(test)]
mod tests {
  use super::*;

  fn volume(device: &str) -> Volume {
    Volume {
      block: OwnedObjectPath::default(),
      drive: None,
      device: device.into(),
      label: device.into(),
      model: String::new(),
      size: 0,
      mount_points: Vec::new(),
      removable: false,
      can_power_off: false,
      lock: None,
    }
  }

  #[test]
  fn volumes_sort_by_device() {
    let mut volumes = vec![
      volume("/dev/sdb1"),
      volume("/dev/loop0"),
      volume("/dev/sda2"),
    ];
    sort_volumes(&mut volumes);
    let devices: Vec<_> = volumes.iter().map(|v| v.device.as_str()).collect();
    assert_eq!(devices, ["/dev/loop0", "/dev/sda2", "/dev/sdb1"]);
  }

  #[test]
  fn smart_reads_ata_then_nvme() {
    let mut drive = Drive {
      path: OwnedObjectPath::default(),
      id: String::new(),
      vendor: String::new(),
      model: String::new(),
      revision: String::new(),
      serial: String::new(),
      size: 0,
      media: String::new(),
      media_available: true,
      removable: false,
      ejectable: false,
      can_power_off: false,
      connection_bus: String::new(),
      rotation_rate: 0,
      sort_key: String::new(),
      ata: Some(Ata::default()),
      nvme: None,
    };
    // ata without SMART support
    assert!(drive.smart().is_none());
    drive.ata = Some(Ata {
      smart_supported: true,
      smart_temperature: 313.15,
      smart_power_on_seconds: 7200,
      ..Default::default()
    });
    let smart = drive.smart().unwrap();
    assert!((smart.temperature_celsius.unwrap() - 40.).abs() < 1e-9);
    assert_eq!(smart.power_on_hours, Some(2));
    drive.ata = None;
    drive.nvme = Some(NvmeController {
      smart_critical_warning: vec!["temperature".into()],
      ..Default::default()
    });
    let smart = drive.smart().unwrap();
    assert!(smart.failing);
    assert_eq!(
      (smart.temperature_celsius, smart.power_on_hours),
      (None, None)
    );
  }
}
