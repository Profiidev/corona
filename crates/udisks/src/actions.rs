//! udisksd's methods, one per D-Bus method, on the object at the given path

use std::os::fd::{AsFd, OwnedFd};

use anyhow::Result;
use zbus::zvariant::{Fd, OwnedObjectPath, Value};

use crate::{
  Udisks,
  options::{
    CreatePartitionOptions, DeleteOptions, FormatOptions, LoopSetupOptions, MountOptions,
    PassphraseOptions, ToOptions, UnlockOptions, UnmountOptions, none,
  },
  proxy,
  proxy::{
    AtaProxy, BlockProxy, DriveProxy, EncryptedProxy, FilesystemProxy, JobProxy, LoopProxy,
    ManagerProxy, NvmeControllerProxy, PartitionProxy, PartitionTableProxy, SwapspaceProxy,
  },
  snapshot::read_nvme_health,
  state::{AtaAttribute, NvmeHealth},
};

const MANAGER: &str = "/org/freedesktop/UDisks2/Manager";

fn manager() -> OwnedObjectPath {
  OwnedObjectPath::try_from(MANAGER).expect("a valid path")
}

/// Calls `$call` on a `$proxy` at `$path`, after owning what it borrows as `name = value`
macro_rules! call {
  ($self:ident, $proxy:ident, $path:expr $(, $name:ident = $value:expr)* ; |$p:ident| $call:expr) => {{
    let conn = $self.conn.clone();
    let path = OwnedObjectPath::clone(&$path);
    $(let $name = $value;)*
    async move {
      let $p = crate::at::<$proxy<'static>>(&conn, path).await?;
      Ok::<_, anyhow::Error>($call.await?.into())
    }
  }};
}

impl Udisks {
  // Manager

  /// Whether `kind` can be made, else the missing tool
  pub fn can_format(&self, kind: &str) -> impl Future<Output = Result<(bool, String)>> + use<> {
    call!(self, ManagerProxy, manager(), kind = kind.to_string(); |p| p.can_format(&kind))
  }

  /// Whether `kind` resizes, the `BDFsResizeFlags` mode and else the missing tool
  pub fn can_resize(
    &self,
    kind: &str,
  ) -> impl Future<Output = Result<(bool, u64, String)>> + use<> {
    call!(self, ManagerProxy, manager(), kind = kind.to_string(); |p| p.can_resize(&kind))
  }

  pub fn can_check(&self, kind: &str) -> impl Future<Output = Result<(bool, String)>> + use<> {
    call!(self, ManagerProxy, manager(), kind = kind.to_string(); |p| p.can_check(&kind))
  }

  pub fn can_repair(&self, kind: &str) -> impl Future<Output = Result<(bool, String)>> + use<> {
    call!(self, ManagerProxy, manager(), kind = kind.to_string(); |p| p.can_repair(&kind))
  }

  /// A loop device of the open image, returns its block
  pub fn loop_setup(
    &self,
    file: OwnedFd,
    options: LoopSetupOptions,
  ) -> impl Future<Output = Result<OwnedObjectPath>> + use<> {
    call!(self, ManagerProxy, manager(); |p| p.loop_setup(Fd::from(file.as_fd()), options.options()))
  }

  pub fn get_block_devices(&self) -> impl Future<Output = Result<Vec<OwnedObjectPath>>> + use<> {
    call!(self, ManagerProxy, manager(); |p| p.get_block_devices(none()))
  }

  // Block

  /// Formats as `kind`: a filesystem like `ext4`, `swap`, `empty`, or a table like `gpt`
  pub fn format(
    &self,
    block: &OwnedObjectPath,
    kind: &str,
    options: FormatOptions,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, BlockProxy, block, kind = kind.to_string(); |p| p.format(&kind, options.options()))
  }

  pub fn open_for_backup(
    &self,
    block: &OwnedObjectPath,
  ) -> impl Future<Output = Result<OwnedFd>> + use<> {
    call!(self, BlockProxy, block; |p| p.open_for_backup(none()))
  }

  pub fn open_for_restore(
    &self,
    block: &OwnedObjectPath,
  ) -> impl Future<Output = Result<OwnedFd>> + use<> {
    call!(self, BlockProxy, block; |p| p.open_for_restore(none()))
  }

  pub fn open_for_benchmark(
    &self,
    block: &OwnedObjectPath,
    writable: bool,
  ) -> impl Future<Output = Result<OwnedFd>> + use<> {
    call!(self, BlockProxy, block; |p| p.open_for_benchmark({
      let mut options = none();
      options.insert("writable", Value::from(writable));
      options
    }))
  }

  pub fn rescan(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, BlockProxy, block; |p| p.rescan(none()))
  }

  // Filesystem

  /// Returns where
  pub fn mount(
    &self,
    block: &OwnedObjectPath,
    options: MountOptions,
  ) -> impl Future<Output = Result<String>> + use<> {
    call!(self, FilesystemProxy, block; |p| p.mount(options.options()))
  }

  pub fn unmount(
    &self,
    block: &OwnedObjectPath,
    options: UnmountOptions,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, FilesystemProxy, block; |p| p.unmount(options.options()))
  }

  pub fn set_label(
    &self,
    block: &OwnedObjectPath,
    label: &str,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, FilesystemProxy, block, label = label.to_string(); |p| p.set_label(&label, none()))
  }

  /// 0 fills the device
  pub fn resize_filesystem(
    &self,
    block: &OwnedObjectPath,
    size: u64,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, FilesystemProxy, block; |p| p.resize(size, none()))
  }

  /// Whether it is consistent
  pub fn check(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<bool>> + use<> {
    call!(self, FilesystemProxy, block; |p| p.check(none()))
  }

  /// Whether it got repaired
  pub fn repair(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<bool>> + use<> {
    call!(self, FilesystemProxy, block; |p| p.repair(none()))
  }

  // Encrypted

  /// Returns the cleartext block
  pub fn unlock(
    &self,
    block: &OwnedObjectPath,
    passphrase: &str,
    options: UnlockOptions,
  ) -> impl Future<Output = Result<OwnedObjectPath>> + use<> {
    call!(self, EncryptedProxy, block, pass = passphrase.to_string(); |p| p.unlock(&pass, options.options()))
  }

  pub fn lock(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, EncryptedProxy, block; |p| p.lock(none()))
  }

  pub fn change_passphrase(
    &self,
    block: &OwnedObjectPath,
    passphrase: &str,
    new_passphrase: &str,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(
      self, EncryptedProxy, block, old = passphrase.to_string(), new = new_passphrase.to_string();
      |p| p.change_passphrase(&old, &new, none())
    )
  }

  pub fn resize_encrypted(
    &self,
    block: &OwnedObjectPath,
    size: u64,
    options: PassphraseOptions,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, EncryptedProxy, block; |p| p.resize(size, options.options()))
  }

  // PartitionTable

  /// `kind` a type GUID or dos code, empty for the default; returns the partition
  pub fn create_partition(
    &self,
    table: &OwnedObjectPath,
    offset: u64,
    size: u64,
    kind: &str,
    name: &str,
    options: CreatePartitionOptions,
  ) -> impl Future<Output = Result<OwnedObjectPath>> + use<> {
    call!(
      self, PartitionTableProxy, table, kind = kind.to_string(), name = name.to_string();
      |p| p.create_partition(offset, size, &kind, &name, options.options())
    )
  }

  #[allow(clippy::too_many_arguments)]
  pub fn create_partition_and_format(
    &self,
    table: &OwnedObjectPath,
    offset: u64,
    size: u64,
    kind: &str,
    name: &str,
    options: CreatePartitionOptions,
    format_type: &str,
    format_options: FormatOptions,
  ) -> impl Future<Output = Result<OwnedObjectPath>> + use<> {
    call!(
      self, PartitionTableProxy, table,
      kind = kind.to_string(), name = name.to_string(), format = format_type.to_string();
      |p| p.create_partition_and_format(
        offset, size, &kind, &name, options.options(), &format, format_options.options(),
      )
    )
  }

  // Partition

  pub fn set_partition_type(
    &self,
    partition: &OwnedObjectPath,
    kind: &str,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, PartitionProxy, partition, kind = kind.to_string(); |p| p.set_type(&kind, none()))
  }

  pub fn set_partition_name(
    &self,
    partition: &OwnedObjectPath,
    name: &str,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, PartitionProxy, partition, name = name.to_string(); |p| p.set_name(&name, none()))
  }

  /// gpt attribute bits, or 0x80 bootable on dos
  pub fn set_partition_flags(
    &self,
    partition: &OwnedObjectPath,
    flags: u64,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, PartitionProxy, partition; |p| p.set_flags(flags, none()))
  }

  pub fn resize_partition(
    &self,
    partition: &OwnedObjectPath,
    size: u64,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, PartitionProxy, partition; |p| p.resize(size, none()))
  }

  pub fn delete_partition(
    &self,
    partition: &OwnedObjectPath,
    options: DeleteOptions,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, PartitionProxy, partition; |p| p.delete(options.options()))
  }

  // Drive

  pub fn eject(&self, drive: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, DriveProxy, drive; |p| p.eject(none()))
  }

  pub fn power_off(&self, drive: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, DriveProxy, drive; |p| p.power_off(none()))
  }

  // Drive.Ata, NVMe.Controller: SMART, read only

  pub fn ata_smart_update(
    &self,
    drive: &OwnedObjectPath,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, AtaProxy, drive; |p| p.smart_update(none()))
  }

  pub fn ata_smart_attributes(
    &self,
    drive: &OwnedObjectPath,
  ) -> impl Future<Output = Result<Vec<AtaAttribute>>> + use<> {
    let attributes = call!(self, AtaProxy, drive; |p| p.smart_get_attributes(none()));
    async move {
      let attributes: Vec<proxy::AtaAttribute> = attributes.await?;
      Ok(
        attributes
          .into_iter()
          .map(
            |(id, name, flags, value, worst, threshold, pretty, pretty_unit, _)| AtaAttribute {
              id,
              name,
              flags,
              value,
              worst,
              threshold,
              pretty,
              pretty_unit,
            },
          )
          .collect(),
      )
    }
  }

  pub fn nvme_smart_update(
    &self,
    drive: &OwnedObjectPath,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, NvmeControllerProxy, drive; |p| p.smart_update(none()))
  }

  /// The SMART/Health log page
  pub fn nvme_smart_attributes(
    &self,
    drive: &OwnedObjectPath,
  ) -> impl Future<Output = Result<NvmeHealth>> + use<> {
    let log = call!(self, NvmeControllerProxy, drive; |p| p.smart_get_attributes(none()));
    async move { Ok(read_nvme_health(&log.await?)) }
  }

  // Loop

  /// Detaches the image
  pub fn loop_delete(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, LoopProxy, block; |p| p.delete(none()))
  }

  /// Detaches the image on its own once nothing uses it
  pub fn loop_set_autoclear(
    &self,
    block: &OwnedObjectPath,
    autoclear: bool,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, LoopProxy, block; |p| p.set_autoclear(autoclear, none()))
  }

  // Swapspace

  pub fn swap_start(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, SwapspaceProxy, block; |p| p.start(none()))
  }

  pub fn swap_stop(&self, block: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, SwapspaceProxy, block; |p| p.stop(none()))
  }

  pub fn swap_set_label(
    &self,
    block: &OwnedObjectPath,
    label: &str,
  ) -> impl Future<Output = Result<()>> + use<> {
    call!(self, SwapspaceProxy, block, label = label.to_string(); |p| p.set_label(&label, none()))
  }

  // Job

  pub fn cancel_job(&self, job: &OwnedObjectPath) -> impl Future<Output = Result<()>> + use<> {
    call!(self, JobProxy, job; |p| p.cancel(none()))
  }
}
