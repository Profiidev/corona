/// What the session menu can ask of the machine
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionAction {
  Logout,
  Suspend,
  Hibernate,
  SuspendThenHibernate,
  Reboot,
  PowerOff,
  RebootToFirmware,
}

impl SessionAction {
  /// Whether this machine lets the action run
  pub fn allowed(self, capabilities: &SessionCapabilities) -> bool {
    match self {
      SessionAction::Logout => true,
      SessionAction::Suspend => capabilities.suspend,
      SessionAction::Hibernate => capabilities.hibernate,
      SessionAction::SuspendThenHibernate => capabilities.suspend_then_hibernate,
      SessionAction::Reboot => capabilities.reboot,
      SessionAction::PowerOff => capabilities.power_off,
      SessionAction::RebootToFirmware => capabilities.reboot_to_firmware,
    }
  }
}

/// What this machine allows, read when the session menu opens
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionCapabilities {
  pub suspend: bool,
  pub hibernate: bool,
  pub suspend_then_hibernate: bool,
  pub reboot: bool,
  pub power_off: bool,
  pub reboot_to_firmware: bool,
  /// empty when the boot loader can't be told the next entry
  pub boot_entries: Vec<String>,
}
