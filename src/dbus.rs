//! D-Bus services, on their own thread (zbus's blocking API manages its own executor — no need
//! to fold an async runtime into the calloop-driven render loop). Two interfaces:
//!
//! - `org.freedesktop.Notifications` — the real desktop notification spec, so any app that sends
//!   a notification (`notify-send`, browsers, etc.) shows up on the corona notification surface.
//!   ponytail: no actions/icons/urgency/signals yet, just summary+body+timeout — add when a real
//!   app needs them.
//! - `com.corona.Widgets` — a private, made-up interface for triggering the OSD and calendar.
//!   Nothing wires real volume/brightness hardware to this yet: that's a `wpctl`/backlight script
//!   calling `busctl --user call com.corona.Widgets ...`, which is the intended integration point.

use std::collections::HashMap;

use zbus::interface;
use zbus::zvariant::OwnedValue;

use crate::events::{ShellEvent, ShellSender};

pub fn spawn(tx: ShellSender) {
  std::thread::spawn(move || {
    if let Err(e) = run(tx) {
      tracing::error!(
        "D-Bus service stopped (maybe another notification daemon already owns the name?): {e:#}"
      );
    }
  });
}

fn run(tx: ShellSender) -> anyhow::Result<()> {
  let notifications = Notifications {
    tx: tx.clone(),
    next_id: 0,
  };
  let widgets = Widgets { tx };

  let conn = zbus::blocking::connection::Builder::session()?
    .serve_at("/org/freedesktop/Notifications", notifications)?
    .serve_at("/com/corona/Widgets", widgets)?
    .build()?;

  // Each well-known name is requested independently and non-fatally: com.corona.Widgets is
  // ours alone and should always succeed, but org.freedesktop.Notifications may already be
  // owned by another daemon (mako, dunst, a shell's built-in one, ...) — don't let losing that
  // race take corona down, just log it and keep serving Widgets.
  if let Err(e) = conn.request_name("com.corona.Widgets") {
    tracing::warn!("failed to claim com.corona.Widgets bus name: {e:#}");
  }
  if let Err(e) = conn.request_name("org.freedesktop.Notifications") {
    tracing::warn!(
      "org.freedesktop.Notifications already owned by another daemon — notifications disabled: {e:#}"
    );
  }

  // The connection's I/O runs on zbus's own background thread; this one just has to stay alive.
  loop {
    std::thread::park();
  }
}

struct Notifications {
  tx: ShellSender,
  next_id: u32,
}

#[interface(name = "org.freedesktop.Notifications")]
impl Notifications {
  #[allow(clippy::too_many_arguments)]
  fn notify(
    &mut self,
    app_name: String,
    replaces_id: u32,
    _app_icon: String,
    summary: String,
    body: String,
    _actions: Vec<String>,
    _hints: HashMap<String, OwnedValue>,
    expire_timeout: i32,
  ) -> u32 {
    let id = if replaces_id != 0 {
      replaces_id
    } else {
      self.next_id += 1;
      self.next_id
    };
    let timeout_ms = if expire_timeout > 0 {
      expire_timeout
    } else {
      5000
    };
    let _ = self.tx.send(ShellEvent::Notify {
      id,
      app_name,
      summary,
      body,
      timeout_ms,
    });
    id
  }

  fn close_notification(&mut self, id: u32) {
    let _ = self.tx.send(ShellEvent::CloseNotification(id));
  }

  fn get_capabilities(&self) -> Vec<String> {
    vec!["body".into()]
  }

  fn get_server_information(&self) -> (String, String, String, String) {
    (
      "corona".into(),
      "corona".into(),
      env!("CARGO_PKG_VERSION").into(),
      "1.2".into(),
    )
  }
}

struct Widgets {
  tx: ShellSender,
}

#[interface(name = "com.corona.Widgets")]
impl Widgets {
  fn show_osd(&mut self, label: String, value: f64) {
    let _ = self.tx.send(ShellEvent::ShowOsd {
      label,
      value: value as f32,
    });
  }

  fn toggle_calendar(&mut self) {
    let _ = self.tx.send(ShellEvent::ToggleCalendar);
  }
}
