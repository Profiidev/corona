//! A private D-Bus daemon, so tests can own well-known names without clashing.

use std::{
  io::{BufRead, BufReader},
  process::{Child, Command, Stdio},
};

use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use tempfile::TempDir;

/// Runs the app until `done`, in real time: bus I/O happens on zbus's own
/// thread, which gpui's test scheduler cannot see. Needs
/// `cx.executor().allow_parking()`.
pub fn wait_until(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) {
  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    cx.run_until_parked();
    if done(cx) {
      return;
    }
    assert!(Instant::now() < deadline, "timed out waiting");
    std::thread::sleep(Duration::from_millis(5));
  }
}

/// Lets in-flight bus traffic land, for asserting that nothing happens.
pub fn settle(cx: &mut TestAppContext) {
  for _ in 0..20 {
    cx.run_until_parked();
    std::thread::sleep(Duration::from_millis(5));
  }
}

pub struct TestBus {
  daemon: Child,
  address: String,
  _dir: TempDir,
}

impl TestBus {
  /// Panics when `dbus-daemon` is not in `PATH`.
  pub fn new() -> Self {
    let dir = tempfile::tempdir().expect("temp dir for the bus");
    let mut daemon = Command::new("dbus-daemon")
      .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
      .arg(format!("--address=unix:dir={}", dir.path().display()))
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()
      .expect("dbus-daemon in PATH");
    let mut address = String::new();
    BufReader::new(daemon.stdout.take().expect("daemon stdout"))
      .read_line(&mut address)
      .expect("bus address");
    let address = address.trim().to_string();
    assert!(!address.is_empty(), "dbus-daemon printed no address");
    Self {
      daemon,
      address,
      _dir: dir,
    }
  }

  pub fn address(&self) -> &str {
    &self.address
  }

  /// A fresh client connection to this bus.
  pub async fn conn(&self) -> zbus::Connection {
    zbus::connection::Builder::address(self.address())
      .expect("bus address parses")
      .build()
      .await
      .expect("connects to the test bus")
  }
}

impl Default for TestBus {
  fn default() -> Self {
    Self::new()
  }
}

impl Drop for TestBus {
  fn drop(&mut self) {
    self.daemon.kill().ok();
    self.daemon.wait().ok();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn connections_see_each_other() {
    let bus = TestBus::new();
    futures_lite::future::block_on(async {
      let a = bus.conn().await;
      let b = bus.conn().await;
      a.request_name("io.corona.Test").await.unwrap();
      let dbus = zbus::fdo::DBusProxy::new(&b).await.unwrap();
      assert!(
        dbus
          .name_has_owner("io.corona.Test".try_into().unwrap())
          .await
          .unwrap()
      );
    });
  }

  #[test]
  fn buses_are_isolated() {
    let one = TestBus::new();
    let two = TestBus::new();
    assert_ne!(one.address(), two.address());
    futures_lite::future::block_on(async {
      one
        .conn()
        .await
        .request_name("io.corona.Test")
        .await
        .unwrap();
      // the same name is free on another bus
      two
        .conn()
        .await
        .request_name("io.corona.Test")
        .await
        .unwrap();
    });
  }
}
