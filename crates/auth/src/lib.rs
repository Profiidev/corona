use anyhow::{Result, bail};
use corona_config::APP_NAME;
use futures_lite::StreamExt;
use gpui_kit::{App, Global, Task};
use pam_client::{Context, ErrorCode, Flag, conv_mock::Conversation};
use zbus::{Connection, proxy, zvariant::OwnedObjectPath};

#[proxy(
  interface = "net.reactivated.Fprint.Manager",
  default_service = "net.reactivated.Fprint",
  default_path = "/net/reactivated/Fprint/Manager"
)]
trait Manager {
  fn get_default_device(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
  interface = "net.reactivated.Fprint.Device",
  default_service = "net.reactivated.Fprint"
)]
trait Device {
  fn claim(&self, username: &str) -> zbus::Result<()>;
  fn release(&self) -> zbus::Result<()>;
  fn verify_start(&self, finger_name: &str) -> zbus::Result<()>;
  fn verify_stop(&self) -> zbus::Result<()>;
  #[zbus(signal)]
  fn verify_status(&self, result: &str, done: bool) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct Auth {
  conn: Connection,
}

impl Global for Auth {}

pub trait AuthExt {
  fn auth(&self) -> &Auth;
}

impl AuthExt for App {
  fn auth(&self) -> &Auth {
    self.global::<Auth>()
  }
}

impl Auth {
  /// `Ok(false)` for a wrong password, `Err` when PAM itself failed
  pub fn password(&self, user: String, password: String, cx: &App) -> Task<Result<bool>> {
    cx.background_executor()
      .spawn(async move { pam(APP_NAME, &user, password) })
  }

  /// One scan of any enrolled finger. `Ok(false)` when it did not match; start
  /// again for the next try
  pub fn fingerprint(&self, user: String) -> impl Future<Output = Result<bool>> + use<> {
    let conn = self.conn.clone();
    async move {
      let path = ManagerProxy::new(&conn).await?.get_default_device().await?;
      let device = DeviceProxy::builder(&conn).path(path)?.build().await?;
      let _ = device.release().await;
      device.claim(&user).await?;
      let matched = verify(&device).await;
      let _ = device.verify_stop().await;
      let _ = device.release().await;
      matched
    }
  }
}

fn pam(service: &str, user: &str, password: String) -> Result<bool> {
  let conversation = Conversation::with_credentials(user, password);
  let mut context = Context::new(service, Some(user), conversation)?;
  match context.authenticate(Flag::NONE) {
    Err(e) if e.code() == ErrorCode::AUTH_ERR => return Ok(false),
    result => result?,
  }
  context.acct_mgmt(Flag::NONE)?;
  Ok(true)
}

async fn verify(device: &DeviceProxy<'_>) -> Result<bool> {
  // subscribed before the scan starts, so no result slips past
  let status = device.receive_verify_status().await?.map(Some);
  // fprintd crashing ends no signal stream, so watch for it leaving the bus
  let gone = device
    .inner()
    .receive_owner_changed()
    .await?
    .filter(Option::is_none)
    .map(|_| None);
  let mut events = status.or(gone);
  device.verify_start("any").await?;
  while let Some(Some(signal)) = events.next().await {
    let args = signal.args()?;
    match (*args.result(), *args.done()) {
      ("verify-match", _) => return Ok(true),
      ("verify-no-match", _) => return Ok(false),
      (result, true) => bail!("fingerprint scan failed: {result}"),
      // retry-scan, swipe-too-short and the like, the scan goes on
      _ => {}
    }
  }
  bail!("fprintd went away")
}

pub fn init(cx: &mut App, conn: &Connection) {
  cx.set_global(Auth { conn: conn.clone() });
}

#[cfg(test)]
mod tests {
  use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
  };

  use corona_utils::test_bus::TestBus;
  use futures_lite::future::{self, block_on};
  use zbus::{
    DBusError, interface, message::Header, object_server::SignalEmitter, zvariant::ObjectPath,
  };

  use super::*;

  const MANAGER: &str = "/net/reactivated/Fprint/Manager";
  const DEVICE: &str = "/net/reactivated/Fprint/Device/0";

  type Calls = Arc<Mutex<Vec<String>>>;
  /// What one scan reports, in order; an empty one never finishes
  type Scan = Vec<(&'static str, bool)>;

  #[derive(Debug, DBusError)]
  #[zbus(prefix = "net.reactivated.Fprint.Error")]
  enum FprintError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NoSuchDevice(String),
    AlreadyInUse(String),
    ClaimDevice(String),
    NoEnrolledPrints(String),
  }

  struct FakeManager {
    device: bool,
  }

  #[interface(name = "net.reactivated.Fprint.Manager")]
  impl FakeManager {
    fn get_default_device(&self) -> Result<ObjectPath<'static>, FprintError> {
      match self.device {
        true => Ok(ObjectPath::from_static_str_unchecked(DEVICE)),
        false => Err(FprintError::NoSuchDevice("No devices available".into())),
      }
    }
  }

  /// Like fprintd, only the connection that claimed the device may use it
  struct FakeDevice {
    calls: Calls,
    claimant: Option<String>,
    enrolled: bool,
    scans: VecDeque<Scan>,
  }

  impl FakeDevice {
    fn new(calls: &Calls, scans: impl IntoIterator<Item = Scan>) -> Self {
      Self {
        calls: calls.clone(),
        claimant: None,
        enrolled: true,
        scans: scans.into_iter().collect(),
      }
    }

    fn record(&self, call: impl Into<String>) {
      self.calls.lock().unwrap().push(call.into());
    }

    fn check_claim(&self, header: &Header<'_>) -> Result<(), FprintError> {
      match self.claimant == header.sender().map(|s| s.to_string()) {
        true => Ok(()),
        false => Err(FprintError::ClaimDevice("Device was not claimed".into())),
      }
    }
  }

  #[interface(name = "net.reactivated.Fprint.Device")]
  impl FakeDevice {
    fn claim(
      &mut self,
      username: &str,
      #[zbus(header)] header: Header<'_>,
    ) -> Result<(), FprintError> {
      self.record(format!("claim {username}"));
      if self.claimant.is_some() {
        return Err(FprintError::AlreadyInUse("Device already in use".into()));
      }
      self.claimant = header.sender().map(|s| s.to_string());
      Ok(())
    }

    fn release(&mut self, #[zbus(header)] header: Header<'_>) -> Result<(), FprintError> {
      self.record("release");
      self.check_claim(&header)?;
      self.claimant = None;
      Ok(())
    }

    async fn verify_start(
      &mut self,
      finger_name: &str,
      #[zbus(header)] header: Header<'_>,
      #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), FprintError> {
      self.record(format!("verify {finger_name}"));
      self.check_claim(&header)?;
      if !self.enrolled {
        return Err(FprintError::NoEnrolledPrints(
          "No fingerprints enrolled".into(),
        ));
      }
      for (result, done) in self.scans.pop_front().unwrap_or_default() {
        Self::verify_status(&emitter, result, done).await?;
      }
      Ok(())
    }

    fn verify_stop(&self, #[zbus(header)] header: Header<'_>) -> Result<(), FprintError> {
      self.record("stop");
      self.check_claim(&header)
    }

    #[zbus(signal)]
    async fn verify_status(
      emitter: &SignalEmitter<'_>,
      result: &str,
      done: bool,
    ) -> zbus::Result<()>;
  }

  /// fprintd on its own connection, `None` for a machine without a reader
  async fn fprintd(bus: &TestBus, device: Option<FakeDevice>) -> zbus::Connection {
    let service = bus.conn().await;
    let server = service.object_server();
    let manager = FakeManager {
      device: device.is_some(),
    };
    server.at(MANAGER, manager).await.unwrap();
    if let Some(device) = device {
      server.at(DEVICE, device).await.unwrap();
    }
    service
      .request_name("net.reactivated.Fprint")
      .await
      .unwrap();
    service
  }

  async fn auth(bus: &TestBus) -> Auth {
    Auth {
      conn: bus.conn().await,
    }
  }

  /// Waits for fprintd to see `call`; bus I/O runs on zbus's own thread
  async fn until(calls: &Calls, call: &str) {
    while !calls.lock().unwrap().iter().any(|c| c == call) {
      future::yield_now().await;
    }
  }

  fn scan(scans: impl IntoIterator<Item = Scan>) -> (Result<bool>, Vec<String>) {
    let bus = TestBus::new();
    let calls = Calls::default();
    block_on(async {
      let _fprintd = fprintd(&bus, Some(FakeDevice::new(&calls, scans))).await;
      let matched = auth(&bus).await.fingerprint("alice".into()).await;
      (matched, calls.lock().unwrap().clone())
    })
  }

  #[test]
  fn unconfigured_pam_service_never_unlocks() {
    // falls back to the `other` service, which denies everything
    let unlocked = pam("corona-test-unconfigured", "root", "hunter2".into());
    assert!(!matches!(unlocked, Ok(true)), "{unlocked:?}");
  }

  #[test]
  fn match_after_retry() {
    let (matched, calls) = scan([vec![("verify-retry-scan", false), ("verify-match", true)]]);
    assert!(matched.unwrap());
    assert_eq!(
      calls,
      ["release", "claim alice", "verify any", "stop", "release"]
    );
  }

  #[test]
  fn no_match() {
    let (matched, calls) = scan([vec![("verify-no-match", true)]]);
    assert!(!matched.unwrap());
    assert_eq!(calls.last().unwrap(), "release");
  }

  #[test]
  fn hints_do_not_end_the_scan() {
    let hints = [
      "verify-swipe-too-short",
      "verify-finger-not-centered",
      "verify-remove-and-retry",
    ];
    let mut scan_ = hints.map(|h| (h, false)).to_vec();
    scan_.push(("verify-match", true));
    assert!(scan([scan_]).0.unwrap());
  }

  #[test]
  fn failed_scan_is_an_error_and_releases() {
    for failure in ["verify-disconnected", "verify-unknown-error"] {
      let (matched, calls) = scan([vec![(failure, true)]]);
      let error = matched.unwrap_err().to_string();
      assert!(error.contains(failure), "{error}");
      assert_eq!(calls[calls.len() - 2..], ["stop", "release"]);
    }
  }

  #[test]
  fn no_enrolled_prints() {
    let bus = TestBus::new();
    let calls = Calls::default();
    let device = FakeDevice {
      enrolled: false,
      ..FakeDevice::new(&calls, [])
    };
    block_on(async {
      let _fprintd = fprintd(&bus, Some(device)).await;
      let error = auth(&bus)
        .await
        .fingerprint("alice".into())
        .await
        .unwrap_err();
      assert!(
        format!("{error:#}").contains("NoEnrolledPrints"),
        "{error:#}"
      );
    });
    // still let go of the device
    assert_eq!(calls.lock().unwrap().last().unwrap(), "release");
  }

  #[test]
  fn no_reader() {
    let bus = TestBus::new();
    block_on(async {
      let _fprintd = fprintd(&bus, None).await;
      let error = auth(&bus)
        .await
        .fingerprint("alice".into())
        .await
        .unwrap_err();
      assert!(format!("{error:#}").contains("NoSuchDevice"), "{error:#}");
    });
  }

  #[test]
  fn no_fprintd() {
    let bus = TestBus::new();
    block_on(async {
      assert!(auth(&bus).await.fingerprint("alice".into()).await.is_err());
    });
  }

  #[test]
  fn claimed_by_someone_else() {
    let bus = TestBus::new();
    let calls = Calls::default();
    let device = FakeDevice::new(&calls, [vec![("verify-match", true)]]);
    block_on(async {
      let _fprintd = fprintd(&bus, Some(device)).await;
      let other = bus.conn().await;
      DeviceProxy::builder(&other)
        .path(DEVICE)
        .unwrap()
        .build()
        .await
        .unwrap()
        .claim("bob")
        .await
        .unwrap();

      let error = auth(&bus)
        .await
        .fingerprint("alice".into())
        .await
        .unwrap_err();
      assert!(format!("{error:#}").contains("AlreadyInUse"), "{error:#}");
    });
    // never scanned for alice
    assert!(
      !calls
        .lock()
        .unwrap()
        .iter()
        .any(|c| c.starts_with("verify"))
    );
  }

  #[test]
  fn fprintd_going_away_ends_the_scan() {
    let bus = TestBus::new();
    let calls = Calls::default();
    // a scan that never reports
    let device = FakeDevice::new(&calls, [vec![]]);
    block_on(async {
      let fprintd = fprintd(&bus, Some(device)).await;
      let auth = auth(&bus).await;
      let (matched, ()) = future::zip(auth.fingerprint("alice".into()), async {
        until(&calls, "verify any").await;
        drop(fprintd);
      })
      .await;
      assert!(matched.unwrap_err().to_string().contains("went away"));
    });
  }

  #[test]
  fn dropped_scan_does_not_block_the_next() {
    let bus = TestBus::new();
    let calls = Calls::default();
    // the first scan never reports, the lock screen gives up on it
    let device = FakeDevice::new(&calls, [vec![], vec![("verify-match", true)]]);
    block_on(async {
      let _fprintd = fprintd(&bus, Some(device)).await;
      let auth = auth(&bus).await;
      let dropped = future::or(
        async { Some(auth.fingerprint("alice".into()).await) },
        async {
          until(&calls, "verify any").await;
          None
        },
      )
      .await;
      assert!(dropped.is_none());
      assert!(auth.fingerprint("alice".into()).await.unwrap());
    });
  }
}
