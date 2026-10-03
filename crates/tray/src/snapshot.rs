use std::{
  collections::HashMap,
  hash::{DefaultHasher, Hash, Hasher},
  path::{Path, PathBuf},
};

use anyhow::Result;
use corona_desktop::entry::icon_for_names;
use corona_utils::error::ErrorLogExt;
use image::RgbaImage;
use zbus::{
  Connection,
  fdo::PropertiesProxy,
  names::InterfaceName,
  proxy::CacheProperties,
  zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::{
  proxy::{ITEM_INTERFACE, Layout, MenuProxy, WatcherProxy, parse_address},
  state::{Category, MenuItem, Status, Toggle, TrayItem, argb_to_rgba, strip_mnemonic},
};

const ICON_SIZE: u16 = 32;

pub async fn snapshot(conn: &Connection) -> Result<Vec<TrayItem>> {
  let watcher = WatcherProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let mut items = Vec::new();
  for address in watcher.registered_status_notifier_items().await? {
    // ponytail: items are read one after another, a hanging one delays the rest by the
    // connection's method timeout
    if let Ok(item) = read_item(conn, &address).await.log_err() {
      items.push(item);
    }
  }
  Ok(items)
}

pub(crate) async fn menu_proxy<'a>(
  conn: &Connection,
  bus: &str,
  path: &OwnedObjectPath,
) -> Result<MenuProxy<'a>> {
  Ok(
    MenuProxy::builder(conn)
      .destination(bus.to_string())?
      .path(path.clone())?
      .cache_properties(CacheProperties::No)
      .build()
      .await?,
  )
}

struct Props(HashMap<String, OwnedValue>);

impl Props {
  fn str(&self, key: &str) -> Option<String> {
    let value: &str = self.0.get(key)?.downcast_ref().ok()?;
    (!value.is_empty()).then(|| value.to_string())
  }

  fn bool(&self, key: &str) -> Option<bool> {
    self.0.get(key)?.downcast_ref().ok()
  }

  fn path(&self, key: &str) -> Option<OwnedObjectPath> {
    let value = self.0.get(key)?.try_clone().ok()?;
    OwnedObjectPath::try_from(value).ok()
  }

  fn pixmap(&self, key: &str) -> Option<RgbaImage> {
    let value = self.0.get(key)?.try_clone().ok()?;
    let pixmaps = Vec::<(i32, i32, Vec<u8>)>::try_from(value).ok()?;
    let (width, height, mut pixels) = pixmaps.into_iter().max_by_key(|(w, _, _)| *w)?;
    argb_to_rgba(&mut pixels);
    RgbaImage::from_raw(width.try_into().ok()?, height.try_into().ok()?, pixels)
  }

  fn tooltip(&self) -> Option<String> {
    let value = self.0.get("ToolTip")?.try_clone().ok()?;
    let (_, _, title, description) =
      <(String, Vec<(i32, i32, Vec<u8>)>, String, String)>::try_from(value).ok()?;
    [title, description].into_iter().find(|s| !s.is_empty())
  }
}

async fn read_item(conn: &Connection, address: &str) -> Result<TrayItem> {
  let (bus, path) = parse_address(address);
  let props = PropertiesProxy::builder(conn)
    .destination(bus.to_string())?
    .path(path)?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .get_all(InterfaceName::from_static_str(ITEM_INTERFACE)?)
    .await?;
  let props = Props(props);

  let status = props
    .str("Status")
    .as_deref()
    .map_or_else(Status::default, Status::from);
  let attention = status == Status::NeedsAttention;
  let theme_path = props.str("IconThemePath");
  let icon = attention
    .then(|| {
      icon(
        &props,
        "AttentionIconName",
        "AttentionIconPixmap",
        theme_path.as_deref(),
      )
    })
    .flatten()
    .or_else(|| icon(&props, "IconName", "IconPixmap", theme_path.as_deref()));

  let menu_path = props.path("Menu").filter(|p| p.as_str() != "/NO_DBUSMENU");
  let menu = match &menu_path {
    Some(path) => read_menu(conn, bus, path)
      .await
      .log_err()
      .unwrap_or_default(),
    None => Vec::new(),
  };

  Ok(TrayItem {
    address: address.to_string(),
    id: props.str("Id").unwrap_or_else(|| bus.to_string()),
    title: props.str("Title"),
    status,
    category: props
      .str("Category")
      .as_deref()
      .map_or_else(Category::default, Category::from),
    icon,
    tooltip: props.tooltip(),
    item_is_menu: props.bool("ItemIsMenu").unwrap_or(false),
    menu,
    menu_path,
  })
}

fn icon(props: &Props, name: &str, pixmap: &str, theme_path: Option<&str>) -> Option<PathBuf> {
  props
    .str(name)
    .and_then(|name| named_icon(&name, theme_path))
    .or_else(|| pixmap_file(&props.pixmap(pixmap)?).log_err().ok())
}

fn named_icon(name: &str, theme_path: Option<&str>) -> Option<PathBuf> {
  if name.starts_with('/') {
    return Some(PathBuf::from(name));
  }
  theme_path
    .and_then(|dir| find_in(Path::new(dir), name, 3))
    .or_else(|| icon_for_names([name], ICON_SIZE))
}

fn find_in(dir: &Path, name: &str, depth: u8) -> Option<PathBuf> {
  let mut subdirs = Vec::new();
  for entry in std::fs::read_dir(dir).ok()?.flatten() {
    let path = entry.path();
    if path.is_dir() {
      subdirs.push(path);
    } else if path.file_stem().is_some_and(|s| s == name)
      && path.extension().is_some_and(|e| e == "png" || e == "svg")
    {
      return Some(path);
    }
  }
  if depth == 0 {
    return None;
  }
  subdirs
    .into_iter()
    .find_map(|d| find_in(&d, name, depth - 1))
}

fn pixmap_file(image: &RgbaImage) -> Result<PathBuf> {
  let mut hasher = DefaultHasher::new();
  image.dimensions().hash(&mut hasher);
  image.as_raw().hash(&mut hasher);
  let path = cache_dir().join(format!("{:016x}.png", hasher.finish()));
  if !path.exists() {
    std::fs::create_dir_all(cache_dir())?;
    image.save(&path)?;
  }
  Ok(path)
}

pub(crate) fn cache_dir() -> PathBuf {
  dirs::runtime_dir()
    .unwrap_or_else(std::env::temp_dir)
    .join("corona")
    .join("tray")
}

async fn read_menu(conn: &Connection, bus: &str, path: &OwnedObjectPath) -> Result<Vec<MenuItem>> {
  let (_, (_, _, children)) = menu_proxy(conn, bus, path)
    .await?
    .get_layout(0, -1, &[])
    .await?;
  Ok(
    children
      .iter()
      .filter_map(|c| menu_item(c).log_err().ok())
      .collect(),
  )
}

fn menu_item(value: &OwnedValue) -> Result<MenuItem> {
  let (id, props, children): Layout = value.try_clone()?.try_into()?;
  let props = Props(props);
  let toggled = props
    .0
    .get("toggle-state")
    .and_then(|v| v.downcast_ref::<i32>().ok())
    == Some(1);

  Ok(MenuItem {
    id,
    label: props
      .str("label")
      .map(|l| strip_mnemonic(&l))
      .unwrap_or_default(),
    separator: props.str("type").as_deref() == Some("separator"),
    enabled: props.bool("enabled").unwrap_or(true),
    visible: props.bool("visible").unwrap_or(true),
    toggle: match props.str("toggle-type").as_deref() {
      Some("checkmark") => Toggle::Checkmark(toggled),
      Some("radio") => Toggle::Radio(toggled),
      _ => Toggle::None,
    },
    icon_name: props.str("icon-name"),
    children: children
      .iter()
      .filter_map(|c| menu_item(c).log_err().ok())
      .collect(),
  })
}

#[cfg(test)]
mod tests {
  /// reads the tray items on this session bus: `cargo test -p corona_tray -- --ignored --nocapture`
  #[test]
  #[ignore]
  fn live_snapshot() {
    zbus::block_on(async {
      let conn = zbus::Connection::session().await.unwrap();
      for item in super::snapshot(&conn).await.unwrap() {
        println!("{item:#?}");
      }
    });
  }
}
