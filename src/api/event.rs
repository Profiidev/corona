use crate::Corona;
pub use crate::event::{dbus::CloseReason, event::*};

impl Corona {
  pub fn signal_notification_closed(&self, id: u32, reason: CloseReason) {
    self.dbus().notification_closed(id, reason);
  }

  pub fn signal_notification_action(&self, id: u32, action: &str) {
    self.dbus().notification_action(id, action);
  }

  pub fn on_event<F: Fn(ShellEvent) + 'static>(&self, f: F) {
    self.add_event_listener(Box::new(f));
  }
}
