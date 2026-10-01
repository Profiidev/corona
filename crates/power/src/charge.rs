#[zbus::proxy(
  interface = "org.freedesktop.UPower.Device",
  default_service = "org.freedesktop.UPower"
)]
pub(crate) trait ChargeThreshold {
  fn enable_charge_threshold(&self, enabled: bool) -> zbus::Result<()>;
}
