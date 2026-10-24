use anyhow::{Result, bail};
use corona_ipc::{IpcCommand, IpcServer, Reply};
use corona_tray::{MenuItem, TrayExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<ListEntries>().register::<Activate>();
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrayEntry {
  pub address: String,
  /// The item's title, then the menu path, like `Discord › Mute`
  pub label: String,
  /// The menu entry's id, none for the item itself
  pub menu: Option<i32>,
}

fn menu_entries(address: &str, path: &str, items: &[MenuItem], out: &mut Vec<TrayEntry>) {
  for item in items
    .iter()
    .filter(|i| i.visible && !i.separator && i.enabled)
  {
    let label = format!("{path} › {}", item.label);
    if item.children.is_empty() {
      out.push(TrayEntry {
        address: address.to_string(),
        label,
        menu: Some(item.id),
      });
    } else {
      menu_entries(address, &label, &item.children, out);
    }
  }
}

/// Every tray item and the entries of its menu
pub fn entries(cx: &App) -> Vec<TrayEntry> {
  let mut out = Vec::new();
  for item in cx.tray().list_items(cx) {
    let title = item
      .title
      .clone()
      .filter(|t| !t.is_empty())
      .unwrap_or_else(|| item.id.clone());
    out.push(TrayEntry {
      address: item.address.clone(),
      label: title.clone(),
      menu: None,
    });
    menu_entries(&item.address, &title, &item.menu, &mut out);
  }
  out
}

pub struct ListEntries;

impl IpcCommand for ListEntries {
  const COMMAND: &'static str = "tray:list";

  type Payload = ();
  type Response = Vec<TrayEntry>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(entries(cx))
  }
}

/// Activates a tray item, or clicks one of its menu entries
pub struct Activate;

impl IpcCommand for Activate {
  const COMMAND: &'static str = "tray:activate";

  type Payload = (String, Option<i32>);
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("tray:activate answers asynchronously")
  }

  fn reply((address, menu): Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let tray = cx.tray();
    match menu {
      Some(id) => Box::pin(tray.menu_click(&address, id, cx)),
      None => Box::pin(tray.activate(&address, 0, 0)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(id: i32, label: &str, children: Vec<MenuItem>) -> MenuItem {
    MenuItem {
      id,
      label: label.into(),
      separator: false,
      enabled: true,
      visible: true,
      toggle: Default::default(),
      icon_name: None,
      children,
    }
  }

  #[test]
  fn flattens_menus() {
    let mut out = Vec::new();
    let mut hidden = item(3, "Hidden", vec![]);
    hidden.visible = false;
    menu_entries(
      "a",
      "App",
      &[
        item(1, "Open", vec![]),
        item(2, "More", vec![item(4, "Quit", vec![])]),
        hidden,
      ],
      &mut out,
    );
    let labels: Vec<_> = out.iter().map(|e| (e.label.as_str(), e.menu)).collect();
    assert_eq!(
      labels,
      [("App › Open", Some(1)), ("App › More › Quit", Some(4))]
    );
  }
}
