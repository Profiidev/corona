use crate as tray;
use crate::TrayExt;
use corona_js::{
  Cx, Glob, Json, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module, json,
};
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Status {
  /// Idle, a tray may hide it.
  Passive,
  Active,
  /// Wants the user's attention, its icon is the attention icon then.
  NeedsAttention,
}

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum Toggle {
  None,
  Checkmark,
  Radio,
}

#[derive(Serialize, Type)]
#[serde(rename = "TrayMenuItem")]
struct MenuItem {
  /// The id `menuClick` takes.
  id: i32,
  label: String,
  separator: bool,
  enabled: bool,
  visible: bool,
  toggle: Toggle,
  /// Whether a checkmark or radio entry is on.
  checked: bool,
  icon_name: Option<String>,
  children: Vec<MenuItem>,
}

#[derive(Serialize, Type)]
#[serde(rename = "TrayItem")]
struct Item {
  /// The id the actions take.
  address: String,
  /// The app's own id, like "discord".
  id: String,
  title: Option<String>,
  status: Status,
  /// Path of an image file, a themed icon or the item's pixmap.
  icon: Option<String>,
  tooltip: Option<String>,
  /// The item only has a menu, show it instead of calling `activate`.
  item_is_menu: bool,
  can_activate: bool,
  menu: Vec<MenuItem>,
}

#[derive(Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum Orientation {
  Vertical,
  Horizontal,
}

impl From<Orientation> for tray::Orientation {
  fn from(orientation: Orientation) -> Self {
    match orientation {
      Orientation::Vertical => tray::Orientation::Vertical,
      Orientation::Horizontal => tray::Orientation::Horizontal,
    }
  }
}

impl From<&tray::MenuItem> for MenuItem {
  fn from(item: &tray::MenuItem) -> Self {
    let (toggle, checked) = match item.toggle {
      tray::Toggle::None => (Toggle::None, false),
      tray::Toggle::Checkmark(on) => (Toggle::Checkmark, on),
      tray::Toggle::Radio(on) => (Toggle::Radio, on),
    };
    Self {
      id: item.id,
      label: item.label.clone(),
      separator: item.separator,
      enabled: item.enabled,
      visible: item.visible,
      toggle,
      checked,
      icon_name: item.icon_name.clone(),
      children: item.children.iter().map(MenuItem::from).collect(),
    }
  }
}

impl From<&tray::TrayItem> for Item {
  fn from(item: &tray::TrayItem) -> Self {
    Self {
      address: item.address.clone(),
      id: item.id.clone(),
      title: item.title.clone(),
      status: match item.status {
        tray::Status::Passive => Status::Passive,
        tray::Status::Active => Status::Active,
        tray::Status::NeedsAttention => Status::NeedsAttention,
      },
      icon: item.icon.as_ref().map(|p| p.display().to_string()),
      tooltip: item.tooltip.clone(),
      item_is_menu: item.item_is_menu,
      can_activate: item.can_activate,
      menu: item.menu.iter().map(MenuItem::from).collect(),
    }
  }
}

type Trays = Glob<tray::Tray>;

/// The primary action, usually a left click. `x`, `y` are screen coordinates.
#[specta]
async fn activate(cx: Cx, tray: Trays, address: String, x: i32, y: i32) -> Throws<()> {
  tray.activate(&address, x, y, &cx).await.into()
}

/// Usually a middle click.
#[specta]
async fn secondary_activate(cx: Cx, tray: Trays, address: String, x: i32, y: i32) -> Throws<()> {
  tray.secondary_activate(&address, x, y, &cx).await.into()
}

/// Asks the app to show its own menu, for items with an empty `menu`.
#[specta]
async fn context_menu(cx: Cx, tray: Trays, address: String, x: i32, y: i32) -> Throws<()> {
  tray.context_menu(&address, x, y, &cx).await.into()
}

#[specta]
async fn scroll(
  cx: Cx,
  tray: Trays,
  address: String,
  delta: i32,
  orientation: Json<Orientation>,
) -> Throws<()> {
  let scroll = tray.scroll(&address, delta, orientation.0.into(), &cx);
  scroll.await.into()
}

/// Call before showing the menu (`id` 0) or a submenu, apps filling it lazily update it then.
#[specta]
async fn about_to_show(cx: Cx, tray: Trays, address: String, id: i32) -> Throws<()> {
  let shown = tray.about_to_show(&address, id, &cx);
  shown.await.into()
}

/// Clicks the menu entry `id`.
#[specta]
async fn menu_click(cx: Cx, tray: Trays, address: String, id: i32) -> Throws<()> {
  let clicked = tray.menu_click(&address, id, &cx);
  clicked.await.into()
}

/// `corona/tray`
pub struct TrayModule;

impl ScriptModule for TrayModule {
  const NAME: &str = "tray";
  const DOC: &str = "System tray items.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listItems",
      "",
      |cx| cx.tray().items.clone(),
      json(|cx| {
        let items = cx.tray().list_items(cx);
        items.iter().map(Item::from).collect::<Vec<_>>()
      }),
    );
    host_module!(m, {
      async: [
        activate,
        secondary_activate,
        context_menu,
        scroll,
        about_to_show,
        menu_click,
      ],
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(id: i32, toggle: tray::Toggle, children: Vec<tray::MenuItem>) -> tray::MenuItem {
    tray::MenuItem {
      id,
      label: format!("item {id}"),
      separator: false,
      enabled: true,
      visible: true,
      toggle,
      icon_name: None,
      submenu: false,
      children,
    }
  }

  #[test]
  fn toggles() {
    let all = [
      (tray::Toggle::None, "none", false),
      (tray::Toggle::Checkmark(false), "checkmark", false),
      (tray::Toggle::Checkmark(true), "checkmark", true),
      (tray::Toggle::Radio(false), "radio", false),
      (tray::Toggle::Radio(true), "radio", true),
    ];
    for (toggle, name, checked) in all {
      let json = serde_json::to_value(MenuItem::from(&item(1, toggle, vec![]))).unwrap();
      assert_eq!(json["toggle"], name);
      assert_eq!(json["checked"], checked);
    }
  }

  #[test]
  fn menus_are_recursive() {
    let menu = item(
      0,
      tray::Toggle::None,
      vec![
        item(
          1,
          tray::Toggle::Radio(true),
          vec![item(3, tray::Toggle::None, vec![])],
        ),
        item(2, tray::Toggle::None, vec![]),
      ],
    );
    let json = serde_json::to_value(MenuItem::from(&menu)).unwrap();
    assert_eq!(json["children"][0]["id"], 1);
    assert_eq!(json["children"][0]["checked"], true);
    assert_eq!(json["children"][0]["children"][0]["label"], "item 3");
    assert_eq!(json["children"][1]["id"], 2);
    assert!(
      json["children"][1]["children"]
        .as_array()
        .unwrap()
        .is_empty()
    );
  }

  #[test]
  fn orientations() {
    let parse = |name: &str| serde_json::from_value::<Orientation>(name.into());
    let from = |name| tray::Orientation::from(parse(name).unwrap());
    assert_eq!(from("vertical"), tray::Orientation::Vertical);
    assert_eq!(from("horizontal"), tray::Orientation::Horizontal);
    assert!(parse("diagonal").is_err());
  }
}
