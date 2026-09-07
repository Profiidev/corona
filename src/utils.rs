use uuid::Uuid;

pub fn display_uuid(monitor: &str) -> Uuid {
  Uuid::new_v5(&Uuid::NAMESPACE_DNS, monitor.as_bytes())
}
