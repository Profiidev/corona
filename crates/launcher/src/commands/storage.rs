//! The trash

use anyhow::Result;
use gpui_kit::{App, AppContext, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Action, Confirm, Icon, Meta, Outcome},
  registry, simple,
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

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      simple(
        "trash:empty",
        |_| Meta {
          title: t!("app.launcher.storage.empty_trash").into(),
          icon: Icon::Tile(IconName::Trash, 0.),
          keywords: vec!["trash".into(), "bin".into(), "recycle".into()],
          ..Default::default()
        },
        |_| {
          vec![
            Action::spawn("empty", t!("app.launcher.storage.empty_trash"), |_, cx| {
              let emptied = cx.background_spawn(async { empty_trash() });
              async move {
                let n = emptied.await?;
                Ok(Outcome::hud(t!("app.launcher.storage.emptied", count = n)))
              }
            })
            .destructive()
            .confirm(Confirm {
              title: t!("app.launcher.storage.empty_trash").into(),
              message: t!("app.launcher.storage.empty_message").into(),
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
    ],
    cx,
  );
}
