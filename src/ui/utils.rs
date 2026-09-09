use uuid::Uuid;

use crate::integration::compositor::types::Workspace;

pub fn display_uuid(monitor: &str) -> Uuid {
  Uuid::new_v5(&Uuid::NAMESPACE_DNS, monitor.as_bytes())
}

impl Workspace {
  pub fn display_id(&self) -> Uuid {
    display_uuid(&self.monitor)
  }
}
