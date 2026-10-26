//! Drives, block devices and their filesystems through UDisks2

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedObjectPath};

use crate::{
  listener::{listener, subscribe},
  options::{ToOptions, none},
  proxy::{AtaProxy, DriveProxy, EncryptedProxy, FilesystemProxy, NvmeControllerProxy},
};

pub use crate::{
  options::{
    CreatePartitionOptions, DeleteOptions, Extra, FormatOptions, LoopSetupOptions, MountOptions,
    PassphraseOptions, UnlockOptions, UnmountOptions,
  },
  state::{
    Ata, AtaAttribute, Block, Drive, Encrypted, Filesystem, Job, JobCompleted, Lock, Loop,
    NvmeController, NvmeHealth, Partition, PartitionTable, Smart, SmartAttributes, Swapspace,
    Volume,
  },
};

mod actions;
mod listener;
mod options;
mod proxy;
mod snapshot;
mod state;

const SERVICE: &str = "org.freedesktop.UDisks2";

/// The errors polkit answers with when the user could authorize the call, or dismissed the
/// prompt: worth a retry
const NOT_AUTHORIZED: [&str; 2] = [
  "org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain",
  "org.freedesktop.UDisks2.Error.NotAuthorizedDismissed",
];

/// The call failed for a missing authorization the user can still give
pub fn is_not_authorized(error: &anyhow::Error) -> bool {
  error
    .chain()
    .any(|e| match e.downcast_ref::<zbus::Error>() {
      Some(zbus::Error::MethodError(name, _, _)) => NOT_AUTHORIZED.contains(&name.as_str()),
      _ => false,
    })
}

#[derive(Clone)]
pub struct Udisks {
  /// udisksd is up
  pub running: Entity<bool>,
  pub drives: Entity<Vec<Drive>>,
  pub blocks: Entity<Vec<Block>>,
  pub jobs: Entity<Vec<Job>>,
  /// The last job that completed, for a toast
  pub job_completed: Entity<Option<JobCompleted>>,
  /// The filesystems and encrypted containers a user mounts, from `blocks`
  pub volumes: Entity<Vec<Volume>>,
  conn: Connection,
}

impl Global for Udisks {}

pub trait UdisksExt {
  fn udisks(&self) -> &Udisks;
}

impl UdisksExt for App {
  fn udisks(&self) -> &Udisks {
    self.global::<Udisks>()
  }
}

/// A proxy at `path`, reading no properties: those come from the snapshot
async fn at<P>(conn: &Connection, path: OwnedObjectPath) -> zbus::Result<P>
where
  P: From<zbus::Proxy<'static>> + zbus::proxy::Defaults,
{
  zbus::proxy::Builder::<P>::new(conn)
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await
}

impl Udisks {
  pub fn running(&self, cx: &App) -> bool {
    *self.running.read(cx)
  }

  pub fn list_drives<'c>(&self, cx: &'c App) -> &'c [Drive] {
    self.drives.read(cx)
  }

  pub fn list_blocks<'c>(&self, cx: &'c App) -> &'c [Block] {
    self.blocks.read(cx)
  }

  pub fn list_jobs<'c>(&self, cx: &'c App) -> &'c [Job] {
    self.jobs.read(cx)
  }

  pub fn job_completed<'c>(&self, cx: &'c App) -> Option<&'c JobCompleted> {
    self.job_completed.read(cx).as_ref()
  }

  pub fn list_volumes<'c>(&self, cx: &'c App) -> &'c [Volume] {
    self.volumes.read(cx)
  }

  pub fn drive<'c>(&self, path: &OwnedObjectPath, cx: &'c App) -> Option<&'c Drive> {
    self.list_drives(cx).iter().find(|d| &d.path == path)
  }

  pub fn block<'c>(&self, path: &OwnedObjectPath, cx: &'c App) -> Option<&'c Block> {
    self.list_blocks(cx).iter().find(|b| &b.path == path)
  }

  /// Unlocks, then mounts the cleartext, returns where
  pub fn unlock_and_mount(
    &self,
    encrypted: &OwnedObjectPath,
    passphrase: String,
  ) -> impl Future<Output = Result<String>> + use<> {
    let (conn, path) = (self.conn.clone(), encrypted.clone());
    async move {
      let cleartext = at::<EncryptedProxy>(&conn, path)
        .await?
        .unlock(&passphrase, none())
        .await?;
      Ok(
        at::<FilesystemProxy>(&conn, cleartext)
          .await?
          .mount(MountOptions::default().options())
          .await?,
      )
    }
  }

  /// Unmounts the cleartext where mounted, then locks
  pub fn unmount_and_lock(
    &self,
    volume: &Volume,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let encrypted = volume
      .encrypted_block(self.list_blocks(cx))
      .context("not an encrypted volume");
    let cleartext = match (&volume.lock, volume.is_mounted()) {
      (Some(Lock::Unlocked(cleartext)), true) => Some(cleartext.clone()),
      _ => None,
    };
    async move {
      let encrypted = encrypted?;
      if let Some(cleartext) = cleartext {
        unmount(&conn, cleartext).await?;
      }
      Ok(
        at::<EncryptedProxy>(&conn, encrypted)
          .await?
          .lock(none())
          .await?,
      )
    }
  }

  /// Unmounts every filesystem of the drive, locks its encrypted ones, then powers it off where
  /// it can and ejects its media otherwise
  pub fn eject_drive(
    &self,
    drive: &OwnedObjectPath,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let blocks = self.list_blocks(cx);
    let mut steps = Vec::new();
    for block in blocks.iter().filter(|b| b.drive.as_ref() == Some(drive)) {
      let cleartext = block
        .encrypted
        .as_ref()
        .and_then(|e| e.cleartext_device.as_ref())
        .and_then(|path| blocks.iter().find(|b| &b.path == path));
      if let Some(cleartext) = cleartext {
        if cleartext.is_mounted() {
          steps.push(Step::Unmount(cleartext.path.clone()));
        }
        steps.push(Step::Lock(block.path.clone()));
      } else if block.is_mounted() {
        steps.push(Step::Unmount(block.path.clone()));
      }
    }
    let path = drive.clone();
    let power_off = self.drive(drive, cx).is_some_and(|d| d.can_power_off);
    async move {
      for step in steps {
        match step {
          Step::Unmount(block) => unmount(&conn, block.clone())
            .await
            .with_context(|| format!("unmounting {}", block.as_str()))?,
          Step::Lock(block) => at::<EncryptedProxy>(&conn, block.clone())
            .await?
            .lock(none())
            .await
            .with_context(|| format!("locking {}", block.as_str()))?,
        }
      }
      let drive = at::<DriveProxy>(&conn, path).await?;
      match power_off {
        true => drive.power_off(none()).await?,
        false => drive.eject(none()).await?,
      }
      Ok(())
    }
  }

  /// Asks the drive for fresh SMART data and reads the drive again with it
  pub fn refresh_smart(
    &self,
    drive: &OwnedObjectPath,
    cx: &App,
  ) -> impl Future<Output = Result<Option<Drive>>> + use<> {
    let (conn, path) = (self.conn.clone(), drive.clone());
    let kind = self.drive(drive, cx).map(SmartKind::of);
    async move {
      match kind {
        Some(SmartKind::Ata) => {
          at::<AtaProxy>(&conn, path.clone())
            .await?
            .smart_update(none())
            .await?
        }
        Some(SmartKind::Nvme) => {
          at::<NvmeControllerProxy>(&conn, path.clone())
            .await?
            .smart_update(none())
            .await?
        }
        Some(SmartKind::None) | None => {}
      }
      snapshot::drive(&conn, path).await
    }
  }

  /// The ATA attributes or the NVMe health log, whichever the drive has
  pub fn smart_attributes(
    &self,
    drive: &OwnedObjectPath,
    cx: &App,
  ) -> impl Future<Output = Result<SmartAttributes>> + use<> {
    let kind = self.drive(drive, cx).map(SmartKind::of);
    let ata = self.ata_smart_attributes(drive);
    let nvme = self.nvme_smart_attributes(drive);
    let path = drive.clone();
    async move {
      match kind {
        Some(SmartKind::Ata) => Ok(SmartAttributes::Ata(ata.await?)),
        Some(SmartKind::Nvme) => Ok(SmartAttributes::Nvme(nvme.await?)),
        _ => anyhow::bail!("no SMART on {}", path.as_str()),
      }
    }
  }
}

enum Step {
  Unmount(OwnedObjectPath),
  Lock(OwnedObjectPath),
}

enum SmartKind {
  Ata,
  Nvme,
  None,
}

impl SmartKind {
  fn of(drive: &Drive) -> Self {
    match (&drive.ata, &drive.nvme) {
      (Some(ata), _) if ata.smart_supported => Self::Ata,
      (_, Some(_)) => Self::Nvme,
      _ => Self::None,
    }
  }
}

async fn unmount(conn: &Connection, block: OwnedObjectPath) -> Result<()> {
  Ok(
    at::<FilesystemProxy>(conn, block)
      .await?
      .unmount(none())
      .await?,
  )
}

impl Udisks {
  /// A state without udisksd behind it, for UI tests: no listener, calls go to `conn` where
  /// nobody answers
  #[cfg(any(test, feature = "test-support"))]
  pub fn fake(
    cx: &mut App,
    conn: &Connection,
    drives: Vec<Drive>,
    blocks: Vec<Block>,
    jobs: Vec<Job>,
  ) {
    let volumes = snapshot::volumes(&blocks, &drives);
    let conn = conn.clone();
    let state = Udisks {
      running: cx.new(|_| true),
      drives: cx.new(|_| drives),
      blocks: cx.new(|_| blocks),
      jobs: cx.new(|_| jobs),
      job_completed: cx.new(|_| None),
      volumes: cx.new(|_| volumes),
      conn,
    };
    cx.set_global(state);
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Udisks {
    running: cx.new(|_| false),
    drives: cx.new(|_| Vec::new()),
    blocks: cx.new(|_| Vec::new()),
    jobs: cx.new(|_| Vec::new()),
    job_completed: cx.new(|_| None),
    volumes: cx.new(|_| Vec::new()),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  cx.set_global(state);

  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
  };

  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{
    ObjectServer,
    fdo::ObjectManager,
    interface,
    object_server::SignalEmitter,
    zvariant::{ObjectPath, OwnedValue},
  };

  use super::*;

  type Calls = Arc<Mutex<Vec<String>>>;

  const ROOT: &str = "/org/freedesktop/UDisks2";
  const DRIVE: &str = "/org/freedesktop/UDisks2/drives/usb";
  const SDB1: &str = "/org/freedesktop/UDisks2/block_devices/sdb1";
  const SDC1: &str = "/org/freedesktop/UDisks2/block_devices/sdc1";
  const DM0: &str = "/org/freedesktop/UDisks2/block_devices/dm_2d0";

  /// `key=value` per option, sorted
  fn describe(options: &HashMap<String, OwnedValue>) -> String {
    let mut pairs: Vec<_> = options
      .iter()
      .map(
        |(k, v)| match (v.downcast_ref::<&str>(), v.downcast_ref::<bool>()) {
          (Ok(s), _) => format!("{k}={s}"),
          (_, Ok(b)) => format!("{k}={b}"),
          _ => format!("{k}=?"),
        },
      )
      .collect();
    pairs.sort();
    pairs.join(",")
  }

  struct MockManager;

  #[derive(Debug, zbus::DBusError)]
  #[zbus(prefix = "org.freedesktop.UDisks2.Error")]
  enum UdisksError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotAuthorizedCanObtain(String),
  }

  #[interface(name = "org.freedesktop.UDisks2.Manager")]
  impl MockManager {
    #[zbus(property)]
    fn version(&self) -> String {
      "2.11.2".into()
    }
  }

  struct MockDrive {
    calls: Calls,
  }

  #[interface(name = "org.freedesktop.UDisks2.Drive")]
  impl MockDrive {
    fn power_off(&self, _options: HashMap<String, OwnedValue>) {
      self.calls.lock().unwrap().push("PowerOff".into());
    }
    fn eject(&self, _options: HashMap<String, OwnedValue>) -> Result<(), UdisksError> {
      self.calls.lock().unwrap().push("Eject".into());
      Err(UdisksError::NotAuthorizedCanObtain("not authorized".into()))
    }
    #[zbus(property)]
    fn vendor(&self) -> String {
      "SanDisk".into()
    }
    #[zbus(property)]
    fn removable(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn can_power_off(&self) -> bool {
      true
    }
  }

  struct MockBlock {
    device: String,
    label: String,
    drive: &'static str,
    backing: &'static str,
  }

  #[interface(name = "org.freedesktop.UDisks2.Block")]
  impl MockBlock {
    #[zbus(property)]
    fn device(&self) -> Vec<u8> {
      let mut bytes = self.device.as_bytes().to_vec();
      bytes.push(0);
      bytes
    }
    #[zbus(property)]
    fn drive(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked(self.drive)
    }
    #[zbus(property)]
    fn id_label(&self) -> String {
      self.label.clone()
    }
    #[zbus(property)]
    fn crypto_backing_device(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked(self.backing)
    }
  }

  struct MockFilesystem {
    calls: Calls,
    name: &'static str,
    mount_points: Vec<Vec<u8>>,
  }

  #[interface(name = "org.freedesktop.UDisks2.Filesystem")]
  impl MockFilesystem {
    async fn mount(
      &mut self,
      options: HashMap<String, OwnedValue>,
      #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
      let at = format!("/run/media/me/{}", self.name);
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Mount {} {}", self.name, describe(&options)));
      self.mount_points = vec![format!("{at}\0").into_bytes()];
      self.mount_points_changed(&emitter).await.unwrap();
      at
    }
    async fn unmount(
      &mut self,
      options: HashMap<String, OwnedValue>,
      #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Unmount {} {}", self.name, describe(&options)));
      self.mount_points.clear();
      self.mount_points_changed(&emitter).await.unwrap();
    }
    #[zbus(property)]
    fn mount_points(&self) -> Vec<Vec<u8>> {
      self.mount_points.clone()
    }
  }

  struct MockEncrypted {
    calls: Calls,
    cleartext: &'static str,
  }

  #[interface(name = "org.freedesktop.UDisks2.Encrypted")]
  impl MockEncrypted {
    async fn unlock(
      &mut self,
      passphrase: String,
      _options: HashMap<String, OwnedValue>,
      #[zbus(object_server)] server: &ObjectServer,
      #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<ObjectPath<'static>> {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Unlock {passphrase}"));
      if passphrase != "pw" {
        return Err(zbus::fdo::Error::Failed("wrong passphrase".into()));
      }
      add_cleartext(server, &self.calls, Vec::new()).await;
      self.cleartext = DM0;
      self.cleartext_device_changed(&emitter).await.unwrap();
      Ok(ObjectPath::from_static_str_unchecked(DM0))
    }
    async fn lock(
      &mut self,
      _options: HashMap<String, OwnedValue>,
      #[zbus(object_server)] server: &ObjectServer,
      #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
      self.calls.lock().unwrap().push("Lock".into());
      server.remove::<MockFilesystem, _>(DM0).await.unwrap();
      server.remove::<MockBlock, _>(DM0).await.unwrap();
      self.cleartext = "/";
      self.cleartext_device_changed(&emitter).await.unwrap();
    }
    #[zbus(property)]
    fn cleartext_device(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked(self.cleartext)
    }
  }

  async fn add_cleartext(server: &ObjectServer, calls: &Calls, mount_points: Vec<Vec<u8>>) {
    let block = MockBlock {
      device: "/dev/dm-0".into(),
      label: "SECRET".into(),
      drive: "/",
      backing: SDC1,
    };
    server.at(DM0, block).await.unwrap();
    let fs = MockFilesystem {
      calls: calls.clone(),
      name: "dm_2d0",
      mount_points,
    };
    server.at(DM0, fs).await.unwrap();
  }

  struct Udisksd {
    conn: Connection,
    calls: Calls,
  }

  impl Udisksd {
    /// A usb stick with a plain and an encrypted partition, mounted and unlocked as asked
    fn start(bus: &TestBus, mounted: bool, unlocked: bool) -> Self {
      let calls = Calls::default();
      let mount = |at: &str| match mounted {
        true => vec![format!("/run/media/me/{at}\0").into_bytes()],
        false => Vec::new(),
      };
      let conn = block_on(async {
        let conn = bus.conn().await;
        let server = conn.object_server();
        server.at(ROOT, ObjectManager).await.unwrap();
        server
          .at("/org/freedesktop/UDisks2/Manager", MockManager)
          .await
          .unwrap();
        let drive = MockDrive {
          calls: calls.clone(),
        };
        server.at(DRIVE, drive).await.unwrap();
        let sdb1 = MockBlock {
          device: "/dev/sdb1".into(),
          label: "STICK".into(),
          drive: DRIVE,
          backing: "/",
        };
        server.at(SDB1, sdb1).await.unwrap();
        let fs = MockFilesystem {
          calls: calls.clone(),
          name: "sdb1",
          mount_points: mount("sdb1"),
        };
        server.at(SDB1, fs).await.unwrap();
        let sdc1 = MockBlock {
          device: "/dev/sdc1".into(),
          label: String::new(),
          drive: DRIVE,
          backing: "/",
        };
        server.at(SDC1, sdc1).await.unwrap();
        let encrypted = MockEncrypted {
          calls: calls.clone(),
          cleartext: if unlocked { DM0 } else { "/" },
        };
        server.at(SDC1, encrypted).await.unwrap();
        if unlocked {
          add_cleartext(server, &calls, mount("dm_2d0")).await;
        }
        conn.request_name(SERVICE).await.unwrap();
        conn
      });
      Self { conn, calls }
    }

    fn calls(&self) -> Vec<String> {
      self.calls.lock().unwrap().clone()
    }
  }

  fn start(cx: &mut TestAppContext, mounted: bool, unlocked: bool) -> (TestBus, Udisksd) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let udisksd = Udisksd::start(&bus, mounted, unlocked);
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    (bus, udisksd)
  }

  fn volumes(cx: &mut TestAppContext) -> Vec<Volume> {
    cx.read(|cx| cx.udisks().list_volumes(cx).to_vec())
  }

  fn path(p: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(p).unwrap()
  }

  #[gpui::test]
  fn reads_and_follows_udisksd(cx: &mut TestAppContext) {
    let (_bus, udisksd) = start(cx, false, false);
    wait_until(cx, |cx| volumes(cx).len() == 2);
    let all = volumes(cx);
    assert_eq!(all[0].label, "STICK");
    assert_eq!(all[0].model, "SanDisk");
    assert!(all[0].removable && all[0].can_power_off && !all[0].is_mounted());
    assert!(all[1].is_locked());
    cx.read(|cx| {
      let udisks = cx.udisks();
      assert!(udisks.running(cx));
      assert_eq!(udisks.list_drives(cx).len(), 1);
      assert_eq!(udisks.list_blocks(cx).len(), 2);
    });

    // options reach udisksd under their names, the change comes back as a signal
    let mount = cx.read(|cx| {
      cx.udisks().mount(
        &path(SDB1),
        MountOptions {
          options: Some("ro".into()),
          ..Default::default()
        },
      )
    });
    assert_eq!(block_on(mount).unwrap(), "/run/media/me/sdb1");
    wait_until(cx, |cx| volumes(cx)[0].is_mounted());
    assert_eq!(volumes(cx)[0].mount_points, ["/run/media/me/sdb1"]);

    let unmount = cx.read(|cx| {
      cx.udisks().unmount(
        &path(SDB1),
        UnmountOptions {
          force: true,
          ..Default::default()
        },
      )
    });
    block_on(unmount).unwrap();
    wait_until(cx, |cx| !volumes(cx)[0].is_mounted());
    assert_eq!(
      udisksd.calls(),
      [
        "Mount sdb1 auth.no_user_interaction=false,options=ro",
        "Unmount sdb1 auth.no_user_interaction=false,force=true"
      ]
    );

    // hotplug: removed objects go
    block_on(
      udisksd
        .conn
        .object_server()
        .remove::<MockFilesystem, _>(SDB1),
    )
    .unwrap();
    wait_until(cx, |cx| volumes(cx).len() == 1);
  }

  #[gpui::test]
  fn unlocks_mounts_and_locks(cx: &mut TestAppContext) {
    let (_bus, udisksd) = start(cx, false, false);
    wait_until(cx, |cx| volumes(cx).len() == 2);
    let wrong = cx.read(|cx| cx.udisks().unlock_and_mount(&path(SDC1), "nope".into()));
    assert!(block_on(wrong).is_err());
    let unlock = cx.read(|cx| cx.udisks().unlock_and_mount(&path(SDC1), "pw".into()));
    assert_eq!(block_on(unlock).unwrap(), "/run/media/me/dm_2d0");
    wait_until(cx, |cx| volumes(cx)[1].is_mounted());
    let volume = volumes(cx)[1].clone();
    assert_eq!(volume.label, "SECRET");
    assert_eq!(volume.lock, Some(Lock::Unlocked(path(DM0))));

    let lock = cx.read(|cx| cx.udisks().unmount_and_lock(&volume, cx));
    block_on(lock).unwrap();
    wait_until(cx, |cx| volumes(cx)[1].is_locked());
    assert_eq!(
      udisksd.calls(),
      [
        "Unlock nope",
        "Unlock pw",
        "Mount dm_2d0 auth.no_user_interaction=false",
        "Unmount dm_2d0 auth.no_user_interaction=false",
        "Lock"
      ]
    );
  }

  #[gpui::test]
  fn ejecting_unmounts_locks_then_powers_off(cx: &mut TestAppContext) {
    let (_bus, udisksd) = start(cx, true, true);
    wait_until(cx, |cx| {
      volumes(cx).iter().all(Volume::is_mounted) && volumes(cx).len() == 2
    });
    let eject = cx.read(|cx| cx.udisks().eject_drive(&path(DRIVE), cx));
    block_on(eject).unwrap();
    assert_eq!(
      udisksd.calls(),
      [
        "Unmount sdb1 auth.no_user_interaction=false",
        "Unmount dm_2d0 auth.no_user_interaction=false",
        "Lock",
        "PowerOff"
      ]
    );
  }

  #[gpui::test]
  fn udisksd_quitting_is_noticed(cx: &mut TestAppContext) {
    let (_bus, udisksd) = start(cx, false, false);
    wait_until(cx, |cx| volumes(cx).len() == 2);
    drop(udisksd);
    wait_until(cx, |cx| {
      cx.read(|cx| !cx.udisks().running(cx) && cx.udisks().list_volumes(cx).is_empty())
    });
  }

  struct MockJob;

  #[interface(name = "org.freedesktop.UDisks2.Job")]
  impl MockJob {
    #[zbus(signal)]
    async fn completed(
      emitter: &SignalEmitter<'_>,
      success: bool,
      message: &str,
    ) -> zbus::Result<()>;
    #[zbus(property)]
    fn operation(&self) -> String {
      "filesystem-mount".into()
    }
    #[zbus(property)]
    fn objects(&self) -> Vec<ObjectPath<'static>> {
      vec![ObjectPath::from_static_str_unchecked(SDB1)]
    }
    #[zbus(property)]
    fn progress(&self) -> f64 {
      0.25
    }
  }

  const JOB: &str = "/org/freedesktop/UDisks2/jobs/1";

  #[gpui::test]
  fn jobs_and_their_completion(cx: &mut TestAppContext) {
    let (_bus, udisksd) = start(cx, false, false);
    wait_until(cx, |cx| volumes(cx).len() == 2);
    let server = udisksd.conn.object_server();
    block_on(server.at(JOB, MockJob)).unwrap();
    wait_until(cx, |cx| cx.read(|cx| cx.udisks().list_jobs(cx).len() == 1));
    let job = cx.read(|cx| cx.udisks().list_jobs(cx)[0].clone());
    assert_eq!(
      (job.operation.as_str(), job.progress),
      ("filesystem-mount", 0.25)
    );
    block_on(async {
      let job = server.interface::<_, MockJob>(JOB).await.unwrap();
      MockJob::completed(job.signal_emitter(), false, "device busy")
        .await
        .unwrap();
      server.remove::<MockJob, _>(JOB).await.unwrap();
    });
    wait_until(cx, |cx| {
      cx.read(|cx| cx.udisks().job_completed(cx).is_some())
    });
    let done = cx.read(|cx| cx.udisks().job_completed(cx).cloned().unwrap());
    assert_eq!(done.job, path(JOB));
    assert_eq!(done.operation, "filesystem-mount");
    assert_eq!(done.objects, [path(SDB1)]);
    assert!(!done.success);
    assert_eq!(done.message, "device busy");
    wait_until(cx, |cx| cx.read(|cx| cx.udisks().list_jobs(cx).is_empty()));
  }

  #[gpui::test]
  fn polkit_refusals_are_told_apart(cx: &mut TestAppContext) {
    let (_bus, _udisksd) = start(cx, false, false);
    wait_until(cx, |cx| volumes(cx).len() == 2);
    let eject = cx.read(|cx| cx.udisks().eject(&path(DRIVE)));
    assert!(is_not_authorized(&block_on(eject).unwrap_err()));
    let wrong = cx.read(|cx| cx.udisks().unlock_and_mount(&path(SDC1), "nope".into()));
    assert!(!is_not_authorized(&block_on(wrong).unwrap_err()));
  }
}
