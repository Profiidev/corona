use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

#[zbus::proxy(
  interface = "org.freedesktop.UPower.PowerProfiles",
  default_service = "org.freedesktop.UPower.PowerProfiles",
  default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
pub(crate) trait PowerProfiles {
  /// `power-saver`, `balanced` or `performance`
  #[zbus(property)]
  fn active_profile(&self) -> zbus::Result<String>;

  #[zbus(property)]
  fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;

  /// one dict per profile, its name under `Profile`
  #[zbus(property)]
  fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;

  /// why performance is held back right now, like `lap-detected`, empty when it is not
  #[zbus(property)]
  fn performance_degraded(&self) -> zbus::Result<String>;
}
