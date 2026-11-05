//! Typed `a{sv}` options for the methods that take more than the standard ones

use zbus::zvariant::Value;

use crate::proxy::Options;

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
}

#[derive(Clone, Debug, Default)]
pub struct MountOptions {
  /// Comma separated, like `ro,noexec`
  pub options: Option<String>,
}

impl ToOptions for MountOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().some("options", self.options.as_deref()).0
  }
}

#[derive(Clone, Debug, Default)]
pub struct UnmountOptions {
  /// Lazy unmount, even while busy
  pub force: bool,
}

impl ToOptions for UnmountOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().flag("force", self.force).0
  }
}

#[derive(Clone, Debug, Default)]
pub struct FormatOptions {
  pub label: Option<String>,
  /// Wraps the filesystem in LUKS
  pub encrypt_passphrase: Option<String>,
  /// `zero`, `ata-secure-erase` or `ata-secure-erase-enhanced`
  pub erase: Option<String>,
  pub update_partition_type: bool,
  pub no_discard: bool,
  /// Removes fstab and crypttab entries of what is on the device
  pub tear_down: bool,
}

impl ToOptions for FormatOptions {
  fn options(&self) -> Options<'_> {
    Dict::new()
      .some("label", self.label.as_deref())
      .some("encrypt.passphrase", self.encrypt_passphrase.as_deref())
      .some("erase", self.erase.as_deref())
      .flag("update-partition-type", self.update_partition_type)
      .flag("no-discard", self.no_discard)
      .flag("tear-down", self.tear_down)
      .0
  }
}

#[derive(Clone, Debug, Default)]
pub struct UnlockOptions {}

impl ToOptions for UnlockOptions {
  fn options(&self) -> Options<'_> {
    none()
  }
}

/// For `Encrypted.Resize`, which has to unlock the key slot
#[derive(Clone, Debug, Default)]
pub struct PassphraseOptions {
  pub passphrase: Option<String>,
}

impl ToOptions for PassphraseOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().some("passphrase", self.passphrase.as_deref()).0
  }
}

#[derive(Clone, Debug, Default)]
pub struct LoopSetupOptions {
  pub read_only: bool,
}

impl ToOptions for LoopSetupOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().flag("read-only", self.read_only).0
  }
}

#[derive(Clone, Debug, Default)]
pub struct CreatePartitionOptions {}

impl ToOptions for CreatePartitionOptions {
  fn options(&self) -> Options<'_> {
    none()
  }
}

/// For deleting a partition
#[derive(Clone, Debug, Default)]
pub struct DeleteOptions {
  /// Removes fstab and crypttab entries of what is on it
  pub tear_down: bool,
}

impl ToOptions for DeleteOptions {
  fn options(&self) -> Options<'_> {
    Dict::new().flag("tear-down", self.tear_down).0
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
      ..Default::default()
    };
    let options = format.options();
    assert_eq!(
      keys(&options),
      [AUTH, "encrypt.passphrase", "erase", "label", "no-discard"]
    );
    assert_eq!(options["label"], Value::from("USB"));
    assert_eq!(options["no-discard"], Value::from(true));
    let loop_setup = LoopSetupOptions { read_only: true };
    let loop_setup = loop_setup.options();
    assert_eq!(keys(&loop_setup), [AUTH, "read-only"]);
  }
}
