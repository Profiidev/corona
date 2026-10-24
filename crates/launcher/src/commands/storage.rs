//! The trash and removable drives

use std::{cell::RefCell, rc::Rc};

use anyhow::Result;
use corona_udisks::Volume;
use gpui_kit::{App, AppContext, Entity, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Accessory, Action, Confirm, Icon, Meta, Outcome, Tone},
  kit::{ListSource, opener, watch},
  registry, simple,
  view::ListView,
};

/// Items in the trash
pub fn trash_count() -> Result<usize> {
  Ok(trash::os_limited::list()?.len())
}

/// Empties the trash, returns how many items went
pub fn empty_trash() -> Result<usize> {
  let items = trash::os_limited::list()?;
  let n = items.len();
  trash::os_limited::purge_all(items)?;
  Ok(n)
}

/// The system bus, connected once
async fn system() -> Result<zbus::Connection> {
  static BUS: async_once::Bus = async_once::Bus::new();
  BUS.get().await
}

mod async_once {
  use std::sync::OnceLock;

  /// The system bus, kept after the first connect
  pub struct Bus(OnceLock<zbus::Connection>);

  impl Bus {
    pub const fn new() -> Self {
      Self(OnceLock::new())
    }

    pub async fn get(&self) -> anyhow::Result<zbus::Connection> {
      if let Some(conn) = self.0.get() {
        return Ok(conn.clone());
      }
      let conn = zbus::Connection::system().await?;
      Ok(self.0.get_or_init(|| conn).clone())
    }
  }
}

pub async fn volumes() -> Result<Vec<Volume>> {
  corona_udisks::volumes(&system().await?).await
}

pub async fn mount(volume: Volume) -> Result<String> {
  corona_udisks::mount(&system().await?, &volume).await
}

pub async fn eject(volume: Volume) -> Result<()> {
  corona_udisks::eject(&system().await?, &volume).await
}

fn size(bytes: u64) -> String {
  let gb = bytes as f64 / 1e9;
  match gb >= 1. {
    true => format!("{gb:.1} GB"),
    false => format!("{:.0} MB", bytes as f64 / 1e6),
  }
}

/// Drives as rows: eject the mounted ones, mount the others
fn drives_view(mounting: bool) -> crate::View {
  type Loaded = Entity<Option<Vec<Volume>>>;
  let loaded: Rc<RefCell<Option<Loaded>>> = Rc::default();
  let (read, write) = (loaded.clone(), loaded);
  let title = match mounting {
    true => t!("app.launcher.storage.mount"),
    false => t!("app.launcher.storage.eject"),
  };
  let source = ListSource::new("drives", move |_, cx| {
    let Some(volumes) = read.borrow().as_ref().and_then(|e| e.read(cx).clone()) else {
      return Vec::new();
    };
    volumes
      .into_iter()
      .filter(|v| v.mount_points.is_empty() == mounting)
      .map(|volume| drive_row(volume, mounting))
      .collect()
  })
  .observe(move |notify, cx| {
    let entity = cx.new(|_| None);
    *write.borrow_mut() = Some(entity.clone());
    let task = cx.background_spawn(volumes());
    let target = entity.clone();
    cx.spawn(async move |cx| {
      let found = task.await.unwrap_or_else(|e| {
        tracing::warn!("drives: {e:#}");
        Vec::new()
      });
      target.update(cx, |v, cx| {
        *v = Some(found);
        cx.notify();
      });
    })
    .detach();
    vec![watch(&entity, &notify, cx)]
  });
  ListView::new(title, t!("app.launcher.storage.search"), Rc::new(source))
    .empty(t!("app.launcher.storage.no_drives"))
    .into_view()
}

fn drive_row(volume: Volume, mounting: bool) -> Rc<dyn crate::Command> {
  let v = volume.clone();
  simple(
    format!("drive:{}", volume.device),
    move |_| Meta {
      title: v.label.clone().into(),
      subtitle: Some(format!("{} · {} · {}", v.device, v.model, size(v.size)).into()),
      icon: Icon::Named(IconName::HardDrive),
      accessories: v
        .mount_points
        .first()
        .map(|m| Accessory::Tag(m.clone().into(), Tone::Ok))
        .into_iter()
        .collect(),
      ..Default::default()
    },
    move |_| {
      let volume = volume.clone();
      match mounting {
        true => vec![Action::spawn(
          "mount",
          t!("app.launcher.storage.mount_one"),
          move |_, cx| {
            let task = cx.background_spawn(mount(volume.clone()));
            async move {
              let at = task.await?;
              Ok(Outcome::hud(t!("app.launcher.storage.mounted", path = at)))
            }
          },
        )],
        false => vec![Action::spawn(
          "eject",
          t!("app.launcher.storage.eject_one"),
          move |_, cx| {
            let task = cx.background_spawn(eject(volume.clone()));
            async move {
              task.await?;
              Ok(Outcome::hud(t!("app.launcher.storage.ejected")))
            }
          },
        )],
      }
    },
  )
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      simple(
        "trash:empty",
        |_| Meta {
          title: t!("app.launcher.storage.empty_trash").into(),
          subtitle: trash_count()
            .ok()
            .map(|n| SharedString::from(t!("app.launcher.storage.items", count = n).to_string())),
          icon: Icon::Tile(IconName::Trash, 0.),
          keywords: vec!["trash".into(), "bin".into(), "recycle".into()],
          ..Default::default()
        },
        |_| {
          let count = trash_count().unwrap_or(0);
          vec![
            Action::sync(
              "empty",
              t!("app.launcher.storage.empty_trash"),
              |_, _, _| {
                let n = empty_trash()?;
                Ok(Outcome::hud(t!("app.launcher.storage.emptied", count = n)))
              },
            )
            .destructive()
            .confirm(Confirm {
              title: t!("app.launcher.storage.empty_trash").into(),
              message: t!("app.launcher.storage.empty_message", count = count).into(),
              button: t!("app.launcher.storage.empty").into(),
              skippable: false,
            }),
          ]
        },
      ),
      simple(
        "trash:open",
        |_| Meta {
          title: t!("app.launcher.storage.open_trash").into(),
          icon: Icon::Tile(IconName::Trash, 30.),
          keywords: vec!["trash".into(), "bin".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync("open", t!("app.launcher.open"), |_, _, cx| {
            cx.open_url("trash:///");
            Ok(Outcome::close())
          })]
        },
      ),
      opener(
        "drives:eject",
        t!("app.launcher.storage.eject"),
        Icon::Tile(IconName::HardDriveUpload, 30.),
        &["usb", "drive", "unmount", "eject"],
        |_| drives_view(false),
      ),
      opener(
        "drives:mount",
        t!("app.launcher.storage.mount"),
        Icon::Tile(IconName::HardDriveDownload, 30.),
        &["usb", "drive", "mount"],
        |_| drives_view(true),
      ),
    ],
    cx,
  );
}
