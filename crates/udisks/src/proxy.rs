//! Method-only proxies for the parts of udisksd corona uses, signatures from udisks 2.11.
//! Properties are read from `GetManagedObjects` in `snapshot`, not through these.

use std::collections::HashMap;

use zbus::{
  proxy,
  zvariant::{Fd, OwnedFd, OwnedObjectPath, OwnedValue, Value},
};

/// The `a{sv}` every method ends in
pub(crate) type Options<'a> = HashMap<&'a str, Value<'a>>;

#[proxy(
  interface = "org.freedesktop.UDisks2.Manager",
  default_service = "org.freedesktop.UDisks2",
  default_path = "/org/freedesktop/UDisks2/Manager"
)]
pub(crate) trait Manager {
  fn can_format(&self, kind: &str) -> zbus::Result<(bool, String)>;
  fn can_resize(&self, kind: &str) -> zbus::Result<(bool, u64, String)>;
  fn can_check(&self, kind: &str) -> zbus::Result<(bool, String)>;
  fn can_repair(&self, kind: &str) -> zbus::Result<(bool, String)>;
  fn loop_setup(&self, fd: Fd<'_>, options: Options<'_>) -> zbus::Result<OwnedObjectPath>;
  fn get_block_devices(&self, options: Options<'_>) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Block",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Block {
  fn format(&self, kind: &str, options: Options<'_>) -> zbus::Result<()>;
  fn open_for_backup(&self, options: Options<'_>) -> zbus::Result<OwnedFd>;
  fn open_for_restore(&self, options: Options<'_>) -> zbus::Result<OwnedFd>;
  fn open_for_benchmark(&self, options: Options<'_>) -> zbus::Result<OwnedFd>;
  fn rescan(&self, options: Options<'_>) -> zbus::Result<()>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Filesystem",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Filesystem {
  fn set_label(&self, label: &str, options: Options<'_>) -> zbus::Result<()>;
  fn mount(&self, options: Options<'_>) -> zbus::Result<String>;
  fn unmount(&self, options: Options<'_>) -> zbus::Result<()>;
  fn resize(&self, size: u64, options: Options<'_>) -> zbus::Result<()>;
  fn check(&self, options: Options<'_>) -> zbus::Result<bool>;
  fn repair(&self, options: Options<'_>) -> zbus::Result<bool>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Encrypted",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Encrypted {
  fn unlock(&self, passphrase: &str, options: Options<'_>) -> zbus::Result<OwnedObjectPath>;
  fn lock(&self, options: Options<'_>) -> zbus::Result<()>;
  fn change_passphrase(
    &self,
    passphrase: &str,
    new_passphrase: &str,
    options: Options<'_>,
  ) -> zbus::Result<()>;
  fn resize(&self, size: u64, options: Options<'_>) -> zbus::Result<()>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.PartitionTable",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait PartitionTable {
  fn create_partition(
    &self,
    offset: u64,
    size: u64,
    kind: &str,
    name: &str,
    options: Options<'_>,
  ) -> zbus::Result<OwnedObjectPath>;
  #[allow(clippy::too_many_arguments)]
  fn create_partition_and_format(
    &self,
    offset: u64,
    size: u64,
    kind: &str,
    name: &str,
    options: Options<'_>,
    format_type: &str,
    format_options: Options<'_>,
  ) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Partition",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Partition {
  fn set_type(&self, kind: &str, options: Options<'_>) -> zbus::Result<()>;
  fn set_name(&self, name: &str, options: Options<'_>) -> zbus::Result<()>;
  fn set_flags(&self, flags: u64, options: Options<'_>) -> zbus::Result<()>;
  fn resize(&self, size: u64, options: Options<'_>) -> zbus::Result<()>;
  fn delete(&self, options: Options<'_>) -> zbus::Result<()>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Drive",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Drive {
  fn eject(&self, options: Options<'_>) -> zbus::Result<()>;
  fn power_off(&self, options: Options<'_>) -> zbus::Result<()>;
}

/// `(id, name, flags, value, worst, threshold, pretty, pretty_unit, expansion)`
pub(crate) type AtaAttribute = (
  u8,
  String,
  u16,
  i32,
  i32,
  i32,
  i64,
  i32,
  HashMap<String, OwnedValue>,
);

#[proxy(
  interface = "org.freedesktop.UDisks2.Drive.Ata",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Ata {
  fn smart_update(&self, options: Options<'_>) -> zbus::Result<()>;
  fn smart_get_attributes(&self, options: Options<'_>) -> zbus::Result<Vec<AtaAttribute>>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.NVMe.Controller",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait NvmeController {
  fn smart_update(&self, options: Options<'_>) -> zbus::Result<()>;
  fn smart_get_attributes(&self, options: Options<'_>)
  -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Loop",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Loop {
  fn delete(&self, options: Options<'_>) -> zbus::Result<()>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Swapspace",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Swapspace {
  fn start(&self, options: Options<'_>) -> zbus::Result<()>;
  fn stop(&self, options: Options<'_>) -> zbus::Result<()>;
  fn set_label(&self, label: &str, options: Options<'_>) -> zbus::Result<()>;
}

#[proxy(
  interface = "org.freedesktop.UDisks2.Job",
  default_service = "org.freedesktop.UDisks2"
)]
pub(crate) trait Job {
  fn cancel(&self, options: Options<'_>) -> zbus::Result<()>;
}
