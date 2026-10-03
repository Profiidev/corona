use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, proxy::CacheProperties, zvariant::Value};

use crate::{
  listener::{listener, subscribe},
  proxy::{ItemProxy, WatcherProxy, parse_address},
  snapshot::{cache_dir, menu_proxy},
};

pub use crate::state::{Category, MenuItem, Status, Toggle, TrayItem};

mod listener;
mod proxy;
mod snapshot;
mod state;
mod watcher;

#[derive(Clone)]
pub struct Tray {
  pub items: Entity<Vec<TrayItem>>,
  conn: Connection,
}

impl Global for Tray {}

pub trait TrayExt {
  fn tray(&self) -> &Tray;
}

impl TrayExt for App {
  fn tray(&self) -> &Tray {
    self.global::<Tray>()
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
  Vertical,
  Horizontal,
}

impl Tray {
  pub fn list_items<'c>(&self, cx: &'c App) -> &'c [TrayItem] {
    self.items.read(cx)
  }

  pub fn item<'c>(&self, address: &str, cx: &'c App) -> Option<&'c TrayItem> {
    self.list_items(cx).iter().find(|i| i.address == address)
  }

  async fn item_proxy(conn: &Connection, address: &str) -> Result<ItemProxy<'static>> {
    let (bus, path) = parse_address(address);
    Ok(
      ItemProxy::builder(conn)
        .destination(bus.to_string())?
        .path(path)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?,
    )
  }

  pub fn activate(
    &self,
    address: &str,
    x: i32,
    y: i32,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, address) = (self.conn.clone(), address.to_string());
    async move {
      Ok(
        Self::item_proxy(&conn, &address)
          .await?
          .activate(x, y)
          .await?,
      )
    }
  }

  pub fn secondary_activate(
    &self,
    address: &str,
    x: i32,
    y: i32,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, address) = (self.conn.clone(), address.to_string());
    async move {
      Ok(
        Self::item_proxy(&conn, &address)
          .await?
          .secondary_activate(x, y)
          .await?,
      )
    }
  }

  pub fn context_menu(
    &self,
    address: &str,
    x: i32,
    y: i32,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, address) = (self.conn.clone(), address.to_string());
    async move {
      Ok(
        Self::item_proxy(&conn, &address)
          .await?
          .context_menu(x, y)
          .await?,
      )
    }
  }

  pub fn scroll(
    &self,
    address: &str,
    delta: i32,
    orientation: Orientation,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, address) = (self.conn.clone(), address.to_string());
    let orientation = match orientation {
      Orientation::Vertical => "vertical",
      Orientation::Horizontal => "horizontal",
    };
    async move {
      Ok(
        Self::item_proxy(&conn, &address)
          .await?
          .scroll(delta, orientation)
          .await?,
      )
    }
  }

  pub fn about_to_show(
    &self,
    address: &str,
    id: i32,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let target = self.menu_target(address, cx);
    async move {
      let (bus, path) = target?;
      menu_proxy(&conn, &bus, &path)
        .await?
        .about_to_show(id)
        .await?;
      Ok(())
    }
  }

  pub fn menu_click(
    &self,
    address: &str,
    id: i32,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let target = self.menu_target(address, cx);
    async move {
      let (bus, path) = target?;
      let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_secs() as u32);
      menu_proxy(&conn, &bus, &path)
        .await?
        .event(id, "clicked", &Value::I32(0), timestamp)
        .await?;
      Ok(())
    }
  }

  fn menu_target(
    &self,
    address: &str,
    cx: &App,
  ) -> Result<(String, zbus::zvariant::OwnedObjectPath)> {
    let item = self.item(address, cx).context("unknown tray item")?;
    let path = item
      .menu_path
      .clone()
      .context("the tray item has no menu")?;
    Ok((parse_address(address).0.to_string(), path))
  }
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let _ = std::fs::remove_dir_all(cache_dir());

  watcher::serve(conn, cx).await?;

  let state = Tray {
    items: cx.new(|_| Vec::new()),
    conn: conn.clone(),
  };

  let changes = subscribe(conn).await?;
  let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
  conn.request_name(host.as_str()).await?;
  WatcherProxy::new(conn)
    .await?
    .register_status_notifier_host(&host)
    .await?;

  listener(cx, conn.clone(), changes, state.clone());
  cx.set_global(state);
  Ok(())
}
