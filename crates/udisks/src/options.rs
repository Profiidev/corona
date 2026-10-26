//! Typed `a{sv}` options for the methods that take more than the standard ones. Every struct
//! has `extra` for keys not typed here, it wins over a typed key of the same name.

use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::proxy::Options;

pub type Extra = HashMap<String, OwnedValue>;

/// Lets polkit prompt for a call that needs it, rather than fail it
const AUTH: &str = "auth.no_user_interaction";

pub(crate) trait ToOptions {
  fn options(&self) -> Options<'_>;
}

/// The options of a method without any of its own
pub(crate) fn none() -> Options<'static> {
  Dict::new().0
}

/// Collects keys, leaving out unset ones
struct Dict<'a>(Options<'a>);

impl<'a> Dict<'a> {
  fn new() -> Self {
    Self(Options::from([(AUTH, false.into())]))
  }

  fn flag(mut self, key: &'a str, on: bool) -> Self {
    if on {
      self.0.insert(key, true.into());
    }
    self
  }

  fn some<V: Into<Value<'a>>>(mut self, key: &'a str, value: Option<V>) -> Self {
    if let Some(value) = value {
      self.0.insert(key, value.into());
    }
    self
  }

  fn extra(mut self, extra: &'a Extra) -> Options<'a> {
    for (key, value) in extra {
      if let Ok(value) = value.try_clone() {
        self.0.insert(key.as_str(), value.into());
      }
    }
    self.0
  }
}

#[derive(Clone, Debug, Default)]
pub struct MountOptions {
  pub fstype: Option<String>,
  /// Comma separated, like `ro,noexec`
  pub options: Option<String>,
  /// Mounts for this user instead of the caller
  pub as_user: Option<String>,
  pub extra: Extra,
}

impl ToOptions for MountOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("fstype", self.fstype.as_deref())
      .some("options", self.options.as_deref())
      .some("as-user", self.as_user.as_deref())
      .extra(&self.extra)
  }
}

#[derive(Clone, Debug, Default)]
pub struct UnmountOptions {
  /// Lazy unmount, even while busy
  pub force: bool,
  pub extra: Extra,
}

impl ToOptions for UnmountOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().flag("force", self.force).extra(&self.extra)
  }
}

#[derive(Clone, Debug, Default)]
pub struct FormatOptions {
  pub label: Option<String>,
  /// The new filesystem's root belongs to the caller
  pub take_ownership: bool,
  /// Wraps the filesystem in LUKS
  pub encrypt_passphrase: Option<String>,
  /// `luks1` or `luks2`, udisksd's default otherwise
  pub encrypt_type: Option<String>,
  /// `zero`, `ata-secure-erase` or `ata-secure-erase-enhanced`
  pub erase: Option<String>,
  pub update_partition_type: bool,
  /// Returns once checks passed, the format goes on as a job
  pub no_block: bool,
  pub dry_run_first: bool,
  pub no_discard: bool,
  /// Removes fstab and crypttab entries of what is on the device
  pub tear_down: bool,
  pub mkfs_args: Vec<String>,
  pub extra: Extra,
}

impl ToOptions for FormatOptions {
  fn options(&self) -> Options<'_> {
    let mut dict = Dict::new()
      .some("label", self.label.as_deref())
      .flag("take-ownership", self.take_ownership)
      .some("encrypt.passphrase", self.encrypt_passphrase.as_deref())
      .some("encrypt.type", self.encrypt_type.as_deref())
      .some("erase", self.erase.as_deref())
      .flag("update-partition-type", self.update_partition_type)
      .flag("no-block", self.no_block)
      .flag("dry-run-first", self.dry_run_first)
      .flag("no-discard", self.no_discard)
      .flag("tear-down", self.tear_down);
    if !self.mkfs_args.is_empty() {
      dict = dict.some("mkfs-args", Some(self.mkfs_args.clone()));
    }
    dict.extra(&self.extra)
  }
}

#[derive(Clone, Debug, Default)]
pub struct UnlockOptions {
  /// A key file's contents, the passphrase is then ignored
  pub keyfile_contents: Option<Vec<u8>>,
  pub read_only: bool,
  pub extra: Extra,
}

impl ToOptions for UnlockOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("keyfile_contents", self.keyfile_contents.as_deref())
      .flag("read-only", self.read_only)
      .extra(&self.extra)
  }
}

/// For `Encrypted.Resize`, which has to unlock the key slot
#[derive(Clone, Debug, Default)]
pub struct PassphraseOptions {
  pub passphrase: Option<String>,
  pub keyfile_contents: Option<Vec<u8>>,
  pub extra: Extra,
}

impl ToOptions for PassphraseOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("passphrase", self.passphrase.as_deref())
      .some("keyfile_contents", self.keyfile_contents.as_deref())
      .extra(&self.extra)
  }
}

#[derive(Clone, Debug, Default)]
pub struct LoopSetupOptions {
  /// Bytes into the file
  pub offset: Option<u64>,
  /// Bytes of the file, the rest otherwise
  pub size: Option<u64>,
  pub read_only: bool,
  pub no_part_scan: bool,
  pub sector_size: Option<u64>,
  pub extra: Extra,
}

impl ToOptions for LoopSetupOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("offset", self.offset)
      .some("size", self.size)
      .flag("read-only", self.read_only)
      .flag("no-part-scan", self.no_part_scan)
      .some("sector-size", self.sector_size)
      .extra(&self.extra)
  }
}

#[derive(Clone, Debug, Default)]
pub struct CreatePartitionOptions {
  /// `primary`, `extended` or `logical` on dos tables
  pub partition_type: Option<String>,
  pub extra: Extra,
}

impl ToOptions for CreatePartitionOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("partition-type", self.partition_type.as_deref())
      .extra(&self.extra)
  }
}

/// For deleting a partition
#[derive(Clone, Debug, Default)]
pub struct DeleteOptions {
  /// Removes fstab and crypttab entries of what is on it
  pub tear_down: bool,
  pub extra: Extra,
}

impl ToOptions for DeleteOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .flag("tear-down", self.tear_down)
      .extra(&self.extra)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn keys(options: &Options<'_>) -> Vec<String> {
    let mut keys: Vec<_> = options.keys().map(|k| k.to_string()).collect();
    keys.sort();
    keys
  }

  #[test]
  fn unset_keys_are_left_out_but_polkit_may_prompt() {
    for options in [
      none(),
      MountOptions::default().options(),
      FormatOptions::default().options(),
    ] {
      assert_eq!(keys(&options), [AUTH]);
      assert_eq!(options[AUTH], Value::from(false));
    }
  }

  #[test]
  fn typed_keys_use_udisks_names() {
    let format = FormatOptions {
      label: Some("USB".into()),
      encrypt_passphrase: Some("secret".into()),
      erase: Some("zero".into()),
      no_discard: true,
      mkfs_args: vec!["-m0".into()],
      ..Default::default()
    };
    let options = format.options();
    assert_eq!(
      keys(&options),
      [
        AUTH,
        "encrypt.passphrase",
        "erase",
        "label",
        "mkfs-args",
        "no-discard"
      ]
    );
    assert_eq!(options["label"], Value::from("USB"));
    assert_eq!(options["no-discard"], Value::from(true));
    let loop_setup = LoopSetupOptions {
      offset: Some(512),
      read_only: true,
      ..Default::default()
    };
    let loop_setup = loop_setup.options();
    assert_eq!(loop_setup["offset"], Value::from(512u64));
    assert_eq!(keys(&loop_setup), [AUTH, "offset", "read-only"]);
  }

  #[test]
  fn extra_wins() {
    let mount = MountOptions {
      fstype: Some("ext4".into()),
      extra: HashMap::from([
        (
          "fstype".to_string(),
          OwnedValue::try_from(Value::from("vfat")).unwrap(),
        ),
        (AUTH.to_string(), OwnedValue::from(true)),
      ]),
      ..Default::default()
    };
    let options = mount.options();
    assert_eq!(options["fstype"], Value::from("vfat"));
    assert_eq!(options[AUTH], Value::from(true));
  }
}
