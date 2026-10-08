use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures_lite::StreamExt;
use gpui_kit::{App, AppContext};
use tracing::info;
use zbus::{
  Connection, fdo, interface, message::Header, names::BusName, object_server::SignalEmitter,
};

use crate::proxy::{ITEM_PATH, WATCHER_NAME, WATCHER_PATH};

#[derive(Default)]
pub(crate) struct Watcher {
  items: Arc<Mutex<Vec<String>>>,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
  async fn register_status_notifier_item(
    &self,
    service: &str,
    #[zbus(header)] header: Header<'_>,
    #[zbus(connection)] conn: &Connection,
    #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
  ) -> fdo::Result<()> {
    let item = address(service, &header, conn).await?;
    {
      let mut items = self.items.lock().expect("watcher lock");
      if items.contains(&item) {
        return Ok(());
      }
      items.push(item.clone());
    }
    info!("tray item registered: {item}");
    self
      .registered_status_notifier_items_changed(&emitter)
      .await?;
    Self::status_notifier_item_registered(&emitter, &item).await?;
    Ok(())
  }

  /// Hosts are not tracked, corona is always one.
  async fn register_status_notifier_host(&self, _service: &str) {}

  #[zbus(property)]
  fn registered_status_notifier_items(&self) -> Vec<String> {
    self.items.lock().expect("watcher lock").clone()
  }

  #[zbus(property)]
  fn is_status_notifier_host_registered(&self) -> bool {
    true
  }

  #[zbus(property)]
  fn protocol_version(&self) -> i32 {
    0
  }

  #[zbus(signal)]
  async fn status_notifier_item_registered(
    emitter: &SignalEmitter<'_>,
    service: &str,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  async fn status_notifier_item_unregistered(
    emitter: &SignalEmitter<'_>,
    service: &str,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

async fn address(service: &str, header: &Header<'_>, conn: &Connection) -> fdo::Result<String> {
  let sender = || {
    header
      .sender()
      .map(|s| s.to_string())
      .ok_or_else(|| fdo::Error::InvalidArgs("unknown sender".into()))
  };
  if service.starts_with('/') {
    return Ok(format!("{}{service}", sender()?));
  }
  let bus = BusName::try_from(service).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
  let unique = match bus {
    BusName::Unique(unique) => unique.to_string(),
    well_known => fdo::DBusProxy::new(conn)
      .await?
      .get_name_owner(well_known)
      .await?
      .to_string(),
  };
  Ok(format!("{unique}{ITEM_PATH}"))
}

pub(crate) async fn serve(conn: &Connection, cx: &mut App) -> Result<()> {
  let watcher = Watcher::default();
  let items = watcher.items.clone();
  conn.object_server().at(WATCHER_PATH, watcher).await?;
  // queued behind a running watcher: the bus hands corona the name once that one exits
  let reply = conn
    .request_name_with_flags(WATCHER_NAME, fdo::RequestNameFlags::AllowReplacement.into())
    .await?;
  if reply == fdo::RequestNameReply::InQueue {
    info!("another status notifier watcher runs, using it until it exits");
  }

  let mut gone = fdo::DBusProxy::new(conn)
    .await?
    .receive_name_owner_changed()
    .await?;
  let conn = conn.clone();
  cx.background_spawn(async move {
    while let Some(signal) = gone.next().await {
      let Ok(args) = signal.args() else { continue };
      if args.new_owner().is_some() {
        continue;
      }
      let prefix = format!("{}/", args.name());
      let removed: Vec<String> = {
        let mut items = items.lock().expect("watcher lock");
        let removed = items
          .iter()
          .filter(|i| i.starts_with(&prefix))
          .cloned()
          .collect();
        items.retain(|i| !i.starts_with(&prefix));
        removed
      };
      if removed.is_empty() {
        continue;
      }
      let Ok(iface) = conn
        .object_server()
        .interface::<_, Watcher>(WATCHER_PATH)
        .await
      else {
        break;
      };
      let emitter = iface.signal_emitter();
      let _ = iface
        .get()
        .await
        .registered_status_notifier_items_changed(emitter)
        .await;
      for item in removed {
        info!("tray item gone: {item}");
        let _ = Watcher::status_notifier_item_unregistered(emitter, &item).await;
      }
    }
  })
  .detach();

  Ok(())
}
