use corona_tray as tray;
use corona_tray::{Tray, TrayExt};
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Status {
  /// Idle, a tray may hide it.
  Passive,
  Active,
  /// Wants the user's attention, its icon is the attention icon then.
  NeedsAttention,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Toggle {
  None,
  Checkmark,
  Radio,
}

#[derive(Serialize, TS)]
#[ts(rename = "TrayMenuItem")]
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

#[derive(Serialize, TS)]
#[ts(rename = "TrayItem")]
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

#[derive(Deserialize, TS)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Items,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Tray(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.tray();

  Module::new("corona/tray")
    .func(read(
      reads,
      subs,
      "listItems",
      Updates::Items,
      state.items.clone(),
      |cx| {
        let items = cx.tray().list_items(cx);
        items.iter().map(Item::from).collect::<Vec<_>>()
      },
    ))
    .func(named!(
      "activate",
      /// The primary action, usually a left click. `x`, `y` are screen coordinates.
      |tray: Glob<Tray>, address: String, x: i32, y: i32| tray.activate(&address, x, y)
    ))
    .func(named!(
      "secondaryActivate",
      /// Usually a middle click.
      |tray: Glob<Tray>, address: String, x: i32, y: i32| {
        tray.secondary_activate(&address, x, y)
      }
    ))
    .func(named!(
      "contextMenu",
      /// Asks the app to show its own menu, for items with an empty `menu`.
      |tray: Glob<Tray>, address: String, x: i32, y: i32| tray.context_menu(&address, x, y)
    ))
    .func(named!(
      "scroll",
      |tray: Glob<Tray>, address: String, delta: i32, orientation: Orientation| {
        tray.scroll(&address, delta, orientation.into())
      }
    ))
    .func(named!(
      "aboutToShow",
      /// Call before showing the menu (`id` 0) or a submenu, apps filling it lazily update it then.
      |cx: Cx, tray: Glob<Tray>, address: String, id: i32| tray.about_to_show(&address, id, &cx)
    ))
    .func(named!(
      "menuClick",
      /// Clicks the menu entry `id`.
      |cx: Cx, tray: Glob<Tray>, address: String, id: i32| tray.menu_click(&address, id, &cx)
    ))
    .into()
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
