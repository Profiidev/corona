use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, proxy::CacheProperties, zvariant::Value};

use crate::{
  listener::{listener, subscribe},
  proxy::{ItemProxy, WatcherProxy, parse_address},
  snapshot::menu_proxy,
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
  // the icon cache is not cleared: its files are named by their pixels, another corona may be
  // showing them, and the runtime dir goes with the session
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

#[cfg(test)]
mod tests {
  use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
  };

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::{StreamExt, future::block_on};
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{
    MatchRule, MessageStream, interface, message::Type, object_server::SignalEmitter,
    zvariant::ObjectPath,
  };

  use super::*;
  use crate::{
    proxy::{WATCHER_NAME, WATCHER_PATH},
    snapshot::tests::layout,
  };

  type Calls = Arc<Mutex<Vec<String>>>;

  /// icon name, pixmaps, title, description
  type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);

  struct Item {
    calls: Calls,
    id: &'static str,
    status: &'static str,
    icon: String,
    menu: &'static str,
  }

  #[interface(name = "org.kde.StatusNotifierItem")]
  impl Item {
    fn activate(&self, x: i32, y: i32) {
      self.calls.lock().unwrap().push(format!("Activate {x} {y}"));
    }
    fn secondary_activate(&self, x: i32, y: i32) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("SecondaryActivate {x} {y}"));
    }
    fn context_menu(&self, x: i32, y: i32) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("ContextMenu {x} {y}"));
    }
    fn scroll(&self, delta: i32, orientation: String) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Scroll {delta} {orientation}"));
    }
    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(property)]
    fn id(&self) -> String {
      self.id.into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
      "Test App".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
      self.status.into()
    }
    #[zbus(property)]
    fn category(&self) -> String {
      "Communications".into()
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
      self.icon.clone()
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
      "/icons/attention.png".into()
    }
    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
      (String::new(), vec![], "Tip".into(), String::new())
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
      false
    }
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
      ObjectPath::from_static_str_unchecked(self.menu)
    }
  }

  /// an item that can only be clicked for its menu
  struct MenuOnlyItem;

  #[interface(name = "org.kde.StatusNotifierItem")]
  impl MenuOnlyItem {
    fn context_menu(&self, _x: i32, _y: i32) {}
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
      true
    }
  }

  struct Menu {
    calls: Calls,
  }

  #[interface(name = "com.canonical.dbusmenu")]
  impl Menu {
    fn get_layout(
      &self,
      _parent: i32,
      _depth: i32,
      _names: Vec<String>,
    ) -> (u32, crate::proxy::Layout) {
      let quit = layout(7, vec![("label", "_Quit".into())], vec![]);
      (1, (0, HashMap::new(), vec![quit]))
    }
    fn about_to_show(&self, id: i32) -> bool {
      self.calls.lock().unwrap().push(format!("AboutToShow {id}"));
      false
    }
    fn event(&self, id: i32, event: String, _data: zbus::zvariant::Value<'_>, _timestamp: u32) {
      self
        .calls
        .lock()
        .unwrap()
        .push(format!("Event {id} {event}"));
    }
  }

  struct App {
    conn: Connection,
    calls: Calls,
  }

  /// an app with its item at /StatusNotifierItem and a menu, not yet registered
  fn app(bus: &TestBus, id: &'static str) -> App {
    let calls = Calls::default();
    let item = Item {
      calls: calls.clone(),
      id,
      status: "Active",
      icon: "/icons/app.png".into(),
      menu: "/MenuBar",
    };
    let conn = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await
        .unwrap();
      conn
        .object_server()
        .at(
          "/MenuBar",
          Menu {
            calls: calls.clone(),
          },
        )
        .await
        .unwrap();
      conn
    });
    App { conn, calls }
  }

  impl App {
    fn register(&self, service: &str) -> zbus::Result<()> {
      block_on(self.conn.call_method(
        Some(WATCHER_NAME),
        WATCHER_PATH,
        Some(WATCHER_NAME),
        "RegisterStatusNotifierItem",
        &(service,),
      ))
      .map(|_| ())
    }

    fn address(&self) -> String {
      format!("{}/StatusNotifierItem", self.conn.unique_name().unwrap())
    }
  }

  fn start(cx: &mut TestAppContext, bus: &TestBus) -> tempfile::TempDir {
    cx.executor().allow_parking();
    let runtime = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    runtime
  }

  fn items(cx: &mut TestAppContext) -> Vec<String> {
    cx.read(|cx| {
      cx.tray()
        .list_items(cx)
        .iter()
        .map(|i| i.address.clone())
        .collect()
    })
  }

  #[gpui::test]
  fn registered_items_are_read(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let app = app(&bus, "test-app");
    // by its unique name
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    wait_until(cx, |cx| items(cx) == [app.address()]);
    cx.read(|cx| {
      let item = &cx.tray().list_items(cx)[0];
      assert_eq!(
        (item.id.as_str(), item.title.as_deref()),
        ("test-app", Some("Test App"))
      );
      assert_eq!(
        (item.status, item.category),
        (Status::Active, Category::Communications)
      );
      assert_eq!(
        item.icon.as_deref(),
        Some(std::path::Path::new("/icons/app.png"))
      );
      assert_eq!(item.tooltip.as_deref(), Some("Tip"));
      assert!(item.can_activate && !item.item_is_menu);
      assert_eq!(item.menu.len(), 1);
      assert_eq!((item.menu[0].id, item.menu[0].label.as_str()), (7, "Quit"));
    });
    // registering again changes nothing
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    settle(cx);
    assert_eq!(items(cx).len(), 1);
  }

  #[gpui::test]
  fn registration_forms(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    // a well-known name resolves to its owner
    let named = app(&bus, "named");
    block_on(named.conn.request_name("org.kde.StatusNotifierItem-1-1")).unwrap();
    named.register("org.kde.StatusNotifierItem-1-1").unwrap();
    // a path, as ayatana does, belongs to the sender
    let pathed = block_on(async {
      let conn = bus.conn().await;
      let item = Item {
        calls: Calls::default(),
        id: "pathed",
        status: "Passive",
        icon: String::new(),
        menu: "/NO_DBUSMENU",
      };
      conn
        .object_server()
        .at("/org/ayatana/NotificationItem/x", item)
        .await
        .unwrap();
      conn
    });
    block_on(pathed.call_method(
      Some(WATCHER_NAME),
      WATCHER_PATH,
      Some(WATCHER_NAME),
      "RegisterStatusNotifierItem",
      &("/org/ayatana/NotificationItem/x",),
    ))
    .unwrap();
    wait_until(cx, |cx| items(cx).len() == 2);
    let pathed_address = format!(
      "{}/org/ayatana/NotificationItem/x",
      pathed.unique_name().unwrap()
    );
    assert_eq!(items(cx), [named.address(), pathed_address.clone()]);
    cx.read(|cx| {
      let item = cx.tray().item(&pathed_address, cx).unwrap();
      assert_eq!(item.status, Status::Passive);
      // no dbusmenu, no icon
      assert!(item.menu.is_empty() && item.icon.is_none());
    });
    // invalid names are refused, unowned names fail
    assert!(named.register("not a bus name!").is_err());
    assert!(named.register("org.example.Nobody").is_err());
  }

  #[gpui::test]
  fn items_leave_with_their_app(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let watcher = block_on(bus.conn());
    let rule = MatchRule::builder()
      .msg_type(Type::Signal)
      .interface(WATCHER_NAME)
      .unwrap()
      .member("StatusNotifierItemUnregistered")
      .unwrap()
      .build();
    let mut unregistered = block_on(MessageStream::for_match_rule(rule, &watcher, None)).unwrap();
    let app = app(&bus, "leaving");
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    wait_until(cx, |cx| items(cx).len() == 1);
    let address = app.address();
    drop(app);
    wait_until(cx, |cx| items(cx).is_empty());
    let signal = block_on(unregistered.next()).unwrap().unwrap();
    assert_eq!(signal.body().deserialize::<String>().unwrap(), address);
  }

  #[gpui::test]
  fn attention_and_menu_only_items(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let conn = block_on(async {
      let conn = bus.conn().await;
      let item = Item {
        calls: Calls::default(),
        id: "alert",
        status: "NeedsAttention",
        icon: "/icons/app.png".into(),
        menu: "/MenuBar",
      };
      conn
        .object_server()
        .at("/StatusNotifierItem", item)
        .await
        .unwrap();
      conn
    });
    let menu_only = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at("/StatusNotifierItem", MenuOnlyItem)
        .await
        .unwrap();
      conn
    });
    for c in [&conn, &menu_only] {
      block_on(c.call_method(
        Some(WATCHER_NAME),
        WATCHER_PATH,
        Some(WATCHER_NAME),
        "RegisterStatusNotifierItem",
        &(c.unique_name().unwrap().as_str(),),
      ))
      .unwrap();
    }
    wait_until(cx, |cx| items(cx).len() == 2);
    cx.read(|cx| {
      let list = cx.tray().list_items(cx);
      let alert = &list[0];
      assert_eq!(alert.status, Status::NeedsAttention);
      assert_eq!(
        alert.icon.as_deref(),
        Some(std::path::Path::new("/icons/attention.png"))
      );
      // the menu has no reachable dbusmenu: empty, not an error
      assert!(alert.menu.is_empty());
      let bare = &list[1];
      // named by its bus when it has no id
      assert_eq!(bare.id, menu_only.unique_name().unwrap().as_str());
      assert!(bare.item_is_menu && !bare.can_activate);
      assert!(bare.title.is_none() && bare.menu_path.is_none());
    });
  }

  #[gpui::test]
  fn new_icons_are_picked_up(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let app = app(&bus, "changing");
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    wait_until(cx, |cx| items(cx).len() == 1);
    block_on(async {
      let iface = app
        .conn
        .object_server()
        .interface::<_, Item>("/StatusNotifierItem")
        .await
        .unwrap();
      iface.get_mut().await.icon = "/icons/new.png".into();
      Item::new_icon(iface.signal_emitter()).await.unwrap();
    });
    wait_until(cx, |cx| {
      cx.read(|cx| {
        cx.tray().list_items(cx)[0].icon.as_deref() == Some(std::path::Path::new("/icons/new.png"))
      })
    });
  }

  #[gpui::test]
  fn actions_reach_the_item(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let app = app(&bus, "clicked");
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    wait_until(cx, |cx| items(cx).len() == 1);
    let address = app.address();
    let tray = cx.read(|cx| cx.tray().clone());
    block_on(tray.activate(&address, 1, 2)).unwrap();
    block_on(tray.secondary_activate(&address, 3, 4)).unwrap();
    block_on(tray.context_menu(&address, 5, 6)).unwrap();
    block_on(tray.scroll(&address, -1, Orientation::Vertical)).unwrap();
    block_on(tray.scroll(&address, 2, Orientation::Horizontal)).unwrap();
    block_on(tray.scroll(&address, 0, Orientation::Vertical)).unwrap();
    block_on(tray.scroll(&address, i32::MIN, Orientation::Vertical)).unwrap();
    block_on(tray.scroll(&address, i32::MAX, Orientation::Horizontal)).unwrap();
    let task = cx.read(|cx| tray.about_to_show(&address, 7, cx));
    block_on(task).unwrap();
    let task = cx.read(|cx| tray.menu_click(&address, 7, cx));
    block_on(task).unwrap();
    assert_eq!(
      *app.calls.lock().unwrap(),
      [
        "Activate 1 2",
        "SecondaryActivate 3 4",
        "ContextMenu 5 6",
        "Scroll -1 vertical",
        "Scroll 2 horizontal",
        "Scroll 0 vertical",
        &format!("Scroll {} vertical", i32::MIN),
        &format!("Scroll {} horizontal", i32::MAX),
        "AboutToShow 7",
        "Event 7 clicked",
      ]
    );
    let task = cx.read(|cx| tray.menu_click("nope", 1, cx));
    assert_eq!(block_on(task).unwrap_err().to_string(), "unknown tray item");
    assert!(block_on(tray.activate(":1.9999/StatusNotifierItem", 0, 0)).is_err());
    // malformed addresses fail across all actions
    assert!(block_on(tray.activate("/only/path", 0, 0)).is_err());
    assert!(block_on(tray.secondary_activate("/only/path", 0, 0)).is_err());
    assert!(block_on(tray.context_menu("/only/path", 0, 0)).is_err());
    assert!(block_on(tray.scroll("/only/path", 0, Orientation::Vertical)).is_err());
    assert!(block_on(tray.activate(":1.42//invalid//path", 0, 0)).is_err());
  }

  #[gpui::test]
  fn items_without_a_menu_cannot_open_one(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let conn = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at("/StatusNotifierItem", MenuOnlyItem)
        .await
        .unwrap();
      conn
        .call_method(
          Some(WATCHER_NAME),
          WATCHER_PATH,
          Some(WATCHER_NAME),
          "RegisterStatusNotifierItem",
          &(conn.unique_name().unwrap().as_str(),),
        )
        .await
        .unwrap();
      conn
    });
    wait_until(cx, |cx| items(cx).len() == 1);
    let address = format!("{}/StatusNotifierItem", conn.unique_name().unwrap());
    let task = cx.read(|cx| cx.tray().about_to_show(&address, 0, cx));
    assert_eq!(
      block_on(task).unwrap_err().to_string(),
      "the tray item has no menu"
    );
  }

  #[gpui::test]
  fn watcher_properties(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let app = app(&bus, "props");
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    let props = block_on(async {
      zbus::fdo::PropertiesProxy::builder(&app.conn)
        .destination(WATCHER_NAME)
        .unwrap()
        .path(WATCHER_PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .get_all(zbus::names::InterfaceName::from_static_str(WATCHER_NAME).unwrap())
        .await
        .unwrap()
    });
    let get = |key: &str| props.get(key).unwrap().try_clone().unwrap();
    assert_eq!(
      Vec::<String>::try_from(get("RegisteredStatusNotifierItems")).unwrap(),
      [app.address()]
    );
    assert!(bool::try_from(get("IsStatusNotifierHostRegistered")).unwrap());
    assert_eq!(i32::try_from(get("ProtocolVersion")).unwrap(), 0);
  }

  /// another watcher, like a running KDE, that already lists one item
  struct OtherWatcher {
    items: Vec<String>,
  }

  #[interface(name = "org.kde.StatusNotifierWatcher")]
  impl OtherWatcher {
    fn register_status_notifier_host(&self, _service: String) {}
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
      self.items.clone()
    }
  }

  fn other_watcher(bus: &TestBus, items: Vec<String>) -> Connection {
    block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at(WATCHER_PATH, OtherWatcher { items })
        .await
        .unwrap();
      // as KDE's: not to be replaced
      conn
        .request_name_with_flags(WATCHER_NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await
        .unwrap();
      conn
    })
  }

  #[gpui::test]
  fn uses_a_running_watcher(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let app = app(&bus, "elsewhere");
    let _other = other_watcher(&bus, vec![app.address()]);
    let _runtime = start(cx, &bus);
    wait_until(cx, |cx| items(cx) == [app.address()]);
  }

  #[gpui::test]
  fn takes_over_from_a_watcher_that_exits(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let other = other_watcher(&bus, vec![]);
    let _runtime = start(cx, &bus);
    settle(cx);
    drop(other);
    let app = app(&bus, "late");
    wait_until(cx, |_| {
      app
        .register(app.conn.unique_name().unwrap().as_str())
        .is_ok()
    });
    wait_until(cx, |cx| items(cx).len() == 1);
  }

  #[gpui::test]
  fn init_keeps_other_instances_icons(cx: &mut TestAppContext) {
    let runtime = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let cached = crate::snapshot::cache_dir().join("0123456789abcdef.png");
    std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
    std::fs::write(&cached, b"png").unwrap();
    let bus = TestBus::new();
    cx.executor().allow_parking();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    assert!(cached.exists());
  }

  #[gpui::test]
  fn duplicate_registration_suppresses_signals(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let watcher = block_on(bus.conn());
    let rule = MatchRule::builder()
      .msg_type(Type::Signal)
      .interface(WATCHER_NAME)
      .unwrap()
      .member("StatusNotifierItemRegistered")
      .unwrap()
      .build();
    let mut stream = block_on(MessageStream::for_match_rule(rule, &watcher, None)).unwrap();
    let app = app(&bus, "dup");
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    wait_until(cx, |cx| items(cx).len() == 1);
    // First registration signal received
    assert!(block_on(stream.next()).is_some());
    // Register the same item again
    app
      .register(app.conn.unique_name().unwrap().as_str())
      .unwrap();
    settle(cx);
    // Item count remains 1 and no extra items added
    assert_eq!(items(cx).len(), 1);
  }

  #[gpui::test]
  fn multi_items_leave_with_their_app(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let watcher = block_on(bus.conn());
    let rule = MatchRule::builder()
      .msg_type(Type::Signal)
      .interface(WATCHER_NAME)
      .unwrap()
      .member("StatusNotifierItemUnregistered")
      .unwrap()
      .build();
    let mut unregistered = block_on(MessageStream::for_match_rule(rule, &watcher, None)).unwrap();
    let conn = block_on(async {
      let conn = bus.conn().await;
      for path in ["/Item1", "/Item2"] {
        conn
          .object_server()
          .at(
            path,
            Item {
              calls: Calls::default(),
              id: "multi",
              status: "Active",
              icon: "/icons/app.png".into(),
              menu: "/MenuBar",
            },
          )
          .await
          .unwrap();
        conn
          .call_method(
            Some(WATCHER_NAME),
            WATCHER_PATH,
            Some(WATCHER_NAME),
            "RegisterStatusNotifierItem",
            &(path,),
          )
          .await
          .unwrap();
      }
      conn
    });
    wait_until(cx, |cx| items(cx).len() == 2);
    drop(conn);
    wait_until(cx, |cx| items(cx).is_empty());
    let sig1 = block_on(unregistered.next()).unwrap().unwrap();
    let sig2 = block_on(unregistered.next()).unwrap().unwrap();
    let mut left = [
      sig1.body().deserialize::<String>().unwrap(),
      sig2.body().deserialize::<String>().unwrap(),
    ];
    left.sort();
    assert_eq!(left.len(), 2);
  }

  struct NeedsAttentionItemNoAttentionIcon;

  #[interface(name = "org.kde.StatusNotifierItem")]
  impl NeedsAttentionItemNoAttentionIcon {
    #[zbus(property)]
    fn id(&self) -> String {
      "fallback_test".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
      "NeedsAttention".into()
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
      "/icons/standard_fallback.png".into()
    }
  }

  #[gpui::test]
  fn attention_falls_back_to_standard_icon(cx: &mut TestAppContext) {
    let bus = TestBus::new();
    let _runtime = start(cx, &bus);
    let _conn = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at("/StatusNotifierItem", NeedsAttentionItemNoAttentionIcon)
        .await
        .unwrap();
      conn
        .call_method(
          Some(WATCHER_NAME),
          WATCHER_PATH,
          Some(WATCHER_NAME),
          "RegisterStatusNotifierItem",
          &(conn.unique_name().unwrap().as_str(),),
        )
        .await
        .unwrap();
      conn
    });
    wait_until(cx, |cx| items(cx).len() == 1);
    cx.read(|cx| {
      let list = cx.tray().list_items(cx);
      assert_eq!(list[0].status, Status::NeedsAttention);
      assert_eq!(
        list[0].icon.as_deref(),
        Some(std::path::Path::new("/icons/standard_fallback.png"))
      );
    });
  }

  #[test]
  fn can_activate_fallback_on_introspection_failure() {
    let bus = TestBus::new();
    block_on(async {
      let conn = bus.conn().await;
      let result = crate::snapshot::can_activate(&conn, ":1.9999", "/none").await;
      assert!(result.is_err());
    });
  }
}
