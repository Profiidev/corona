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
  fdo::{IntrospectableProxy, PropertiesProxy},
  names::InterfaceName,
  proxy::CacheProperties,
  zvariant::{OwnedObjectPath, OwnedValue},
};

use crate::{
  proxy::{ITEM_INTERFACE, Layout, MenuProxy, WatcherProxy, parse_address},
  state::{Category, MenuItem, Status, Toggle, TrayItem, argb_to_rgba, strip_mnemonic},
};

const ICON_SIZE: u16 = 32;

pub type Activatable = HashMap<String, bool>;

pub async fn snapshot(conn: &Connection, activatable: &mut Activatable) -> Result<Vec<TrayItem>> {
  let watcher = WatcherProxy::builder(conn)
    .cache_properties(CacheProperties::No)
    .build()
    .await?;
  let mut items = Vec::new();
  for address in watcher.registered_status_notifier_items().await? {
    if let Ok(item) = read_item(conn, &address, activatable).await.log_err() {
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

async fn can_activate(conn: &Connection, bus: &str, path: &str) -> Result<bool> {
  let xml = IntrospectableProxy::builder(conn)
    .destination(bus.to_string())?
    .path(path.to_string())?
    .cache_properties(CacheProperties::No)
    .build()
    .await?
    .introspect()
    .await?;
  Ok(xml.contains(r#"name="Activate""#))
}

async fn read_item(
  conn: &Connection,
  address: &str,
  activatable: &mut Activatable,
) -> Result<TrayItem> {
  let (bus, path) = parse_address(address);
  let can_activate = match activatable.get(address) {
    Some(&known) => known,
    None => {
      let known = can_activate(conn, bus, &path)
        .await
        .log_err()
        .unwrap_or(true);
      activatable.insert(address.to_string(), known);
      known
    }
  };
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
    can_activate,
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
pub(crate) mod tests {
  use zbus::zvariant::Value;

  use super::*;
  use crate::state::Toggle;

  pub(crate) fn owned(value: Value<'_>) -> OwnedValue {
    value.try_to_owned().unwrap()
  }

  fn props(pairs: Vec<(&str, Value<'_>)>) -> Props {
    Props(
      pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), owned(v)))
        .collect(),
    )
  }

  #[test]
  fn string_bool_and_path_props() {
    let p = props(vec![
      ("Title", "App".into()),
      ("Empty", "".into()),
      ("Number", 5i32.into()),
      ("ItemIsMenu", true.into()),
      (
        "Menu",
        ObjectPath::from_static_str_unchecked("/MenuBar").into(),
      ),
    ]);
    assert_eq!(p.str("Title").as_deref(), Some("App"));
    assert_eq!(p.str("Empty"), None);
    assert_eq!(p.str("Number"), None);
    assert_eq!(p.str("Missing"), None);
    assert_eq!(p.bool("ItemIsMenu"), Some(true));
    assert_eq!(p.bool("Title"), None);
    assert_eq!(p.path("Menu").unwrap().as_str(), "/MenuBar");
    // a string is not an object path
    assert_eq!(p.path("Title"), None);
  }

  use zbus::zvariant::ObjectPath;

  fn pixmaps(list: Vec<(i32, i32, Vec<u8>)>) -> Props {
    props(vec![("IconPixmap", list.into())])
  }

  #[test]
  fn pixmaps_pick_the_widest() {
    let small = (1, 1, vec![0xff, 1, 2, 3]);
    let large = (2, 1, vec![0x80, 4, 5, 6, 0x40, 7, 8, 9]);
    let image = pixmaps(vec![small.clone(), large, small])
      .pixmap("IconPixmap")
      .unwrap();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.as_raw(), &[4, 5, 6, 0x80, 7, 8, 9, 0x40]);
    // broken ones are none
    assert!(pixmaps(vec![]).pixmap("IconPixmap").is_none());
    assert!(
      pixmaps(vec![(-1, 1, vec![0; 4])])
        .pixmap("IconPixmap")
        .is_none()
    );
    assert!(
      pixmaps(vec![(2, 2, vec![0; 4])])
        .pixmap("IconPixmap")
        .is_none()
    );
    assert!(
      props(vec![("IconPixmap", "x".into())])
        .pixmap("IconPixmap")
        .is_none()
    );
  }

  #[test]
  fn tooltips() {
    let tooltip = |title: &str, description: &str| {
      props(vec![(
        "ToolTip",
        (
          "icon",
          Vec::<(i32, i32, Vec<u8>)>::new(),
          title,
          description,
        )
          .into(),
      )])
      .tooltip()
    };
    assert_eq!(tooltip("Title", "Body").as_deref(), Some("Title"));
    assert_eq!(tooltip("", "Body").as_deref(), Some("Body"));
    assert_eq!(tooltip("", ""), None);
    assert_eq!(props(vec![("ToolTip", "plain".into())]).tooltip(), None);
    assert_eq!(props(vec![]).tooltip(), None);
  }

  pub(crate) fn layout(
    id: i32,
    pairs: Vec<(&str, Value<'_>)>,
    children: Vec<OwnedValue>,
  ) -> OwnedValue {
    let props: HashMap<String, OwnedValue> = pairs
      .into_iter()
      .map(|(k, v)| (k.to_string(), owned(v)))
      .collect();
    owned(Value::from((id, props, children)))
  }

  #[test]
  fn menu_items() {
    let check = layout(
      2,
      vec![
        ("label", "_Mute".into()),
        ("toggle-type", "checkmark".into()),
        ("toggle-state", 1i32.into()),
        ("icon-name", "audio-volume-muted".into()),
      ],
      vec![],
    );
    let radio = layout(
      3,
      vec![
        ("toggle-type", "radio".into()),
        ("toggle-state", 0i32.into()),
      ],
      vec![],
    );
    let separator = layout(
      4,
      vec![("type", "separator".into()), ("visible", false.into())],
      vec![],
    );
    let broken = owned("not a layout".into());
    let parent = layout(
      1,
      vec![("label", "Options".into()), ("enabled", false.into())],
      vec![check, radio, separator, broken],
    );
    let item = menu_item(&parent).unwrap();
    assert_eq!(
      (item.id, item.label.as_str(), item.enabled, item.visible),
      (1, "Options", false, true)
    );
    assert_eq!(item.children.len(), 3);
    let check = &item.children[0];
    assert_eq!(
      (check.label.as_str(), check.toggle),
      ("Mute", Toggle::Checkmark(true))
    );
    assert_eq!(check.icon_name.as_deref(), Some("audio-volume-muted"));
    assert_eq!(item.children[1].toggle, Toggle::Radio(false));
    assert!(item.children[2].separator && !item.children[2].visible);
    assert_eq!(item.children[2].label, "");
    assert!(menu_item(&owned(5i32.into())).is_err());
  }

  #[test]
  fn themed_icons_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let write = |path: &str| {
      let path = dir.path().join(path);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, b"").unwrap();
    };
    write("hicolor/22x22/apps/corona-test-app.svg");
    write("hicolor/22x22/apps/corona-test-other.xpm");
    write("a/b/c/d/corona-test-deep.png");
    write("corona-test-top.png");
    assert_eq!(
      find_in(dir.path(), "corona-test-top", 0),
      Some(dir.path().join("corona-test-top.png"))
    );
    assert_eq!(
      find_in(dir.path(), "corona-test-app", 3),
      Some(dir.path().join("hicolor/22x22/apps/corona-test-app.svg"))
    );
    assert_eq!(find_in(dir.path(), "corona-test-app", 2), None);
    assert_eq!(find_in(dir.path(), "corona-test-other", 3), None);
    assert_eq!(find_in(dir.path(), "corona-test-deep", 3), None);
    assert_eq!(find_in(&dir.path().join("missing"), "x", 3), None);
    // absolute names are used as they are, the theme dir first otherwise
    assert_eq!(
      named_icon("/abs/icon.png", None),
      Some(PathBuf::from("/abs/icon.png"))
    );
    assert_eq!(
      named_icon("corona-test-top", dir.path().to_str()),
      Some(dir.path().join("corona-test-top.png"))
    );
    assert_eq!(
      named_icon("corona-test-nowhere-at-all", dir.path().to_str()),
      None
    );
  }

  #[test]
  fn pixmaps_are_cached_as_png() {
    let runtime = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    assert_eq!(cache_dir(), runtime.path().join("corona").join("tray"));
    let image = RgbaImage::from_raw(1, 1, vec![1, 2, 3, 4]).unwrap();
    let path = pixmap_file(&image).unwrap();
    assert!(path.starts_with(cache_dir()));
    assert_eq!(image::open(&path).unwrap().into_rgba8(), image);
    // the same pixels map to the same file, other pixels to another
    assert_eq!(pixmap_file(&image).unwrap(), path);
    let other = RgbaImage::from_raw(1, 1, vec![4, 3, 2, 1]).unwrap();
    assert_ne!(pixmap_file(&other).unwrap(), path);
    // icons fall back to the pixmap when the name finds nothing
    let p = props(vec![
      ("IconName", "corona-test-nowhere-at-all".into()),
      ("IconPixmap", vec![(1i32, 1i32, vec![4u8, 1, 2, 3])].into()),
    ]);
    assert_eq!(icon(&p, "IconName", "IconPixmap", None), Some(path));
    assert_eq!(icon(&props(vec![]), "IconName", "IconPixmap", None), None);
  }
}
