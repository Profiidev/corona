//! Tray items and their menu entries as results

use std::rc::Rc;

use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, Icon, ListView, Meta, Outcome,
  kit::{ListSource, opener, watch},
  registry, simple,
};
use corona_tray::TrayExt;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::commands::tray::{Activate, entries};

pub fn init(cx: &mut App) {
  registry::register(
    opener(
      "tray:search",
      t!("app.launcher.tray.search_title"),
      Icon::Tile(IconName::PanelTopOpen, 260.),
      &["tray", "status", "menu"],
      |_| {
        ListView::new(
          t!("app.launcher.tray.search_title"),
          t!("app.launcher.tray.search"),
          Rc::new(
            ListSource::new("tray", |_, cx| {
              entries(cx)
                .into_iter()
                .map(|entry| {
                  let label: SharedString = entry.label.clone().into();
                  let icon = cx
                    .tray()
                    .item(&entry.address, cx)
                    .and_then(|i| i.icon.clone())
                    .map(|p| Icon::Image(p.into()))
                    .unwrap_or(Icon::Named(IconName::PanelTopOpen));
                  let id = format!("tray:{}:{:?}", entry.address, entry.menu);
                  simple(
                    id,
                    move |_| Meta {
                      title: label.clone(),
                      icon: icon.clone(),
                      ..Default::default()
                    },
                    move |_| {
                      let (address, menu) = (entry.address.clone(), entry.menu);
                      vec![Action::spawn(
                        "activate",
                        t!("app.launcher.tray.activate"),
                        move |_, cx| {
                          let task = Activate::reply((address.clone(), menu), cx);
                          async move {
                            task.await?;
                            Ok(Outcome::close())
                          }
                        },
                      )]
                    },
                  )
                })
                .collect()
            })
            .observe(|notify, cx| {
              let items = cx.tray().items.clone();
              vec![watch(&items, &notify, cx)]
            }),
          ),
        )
        .empty(t!("app.launcher.tray.empty"))
        .into_view()
      },
    ),
    cx,
  );
}
