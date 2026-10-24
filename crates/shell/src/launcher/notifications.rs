//! Searching the notification history and dismissing toasts

use std::rc::Rc;

use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, Command, Detail, DetailBody, Icon, ListView, Meta, Outcome, Section,
  kit::{ListSource, opener, watch},
  registry, simple,
};
use corona_notifications::{Notification, NotificationsExt, strip_markup};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{commands::notification::DismissToasts, i18n};

/// `time` in the local zone, as `pattern` formats it
fn when(time: std::time::SystemTime, pattern: &str) -> String {
  jiff::Timestamp::try_from(time)
    .map(|ts| i18n::format_time(pattern, &ts.to_zoned(jiff::tz::TimeZone::system())))
    .unwrap_or_default()
}

struct NotificationRow(Notification);

impl Command for NotificationRow {
  fn id(&self) -> SharedString {
    format!("notification:{}", self.0.id).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let n = &self.0;
    let body = strip_markup(&n.body);
    Meta {
      title: n.summary.clone().into(),
      subtitle: (!body.is_empty()).then(|| body.clone().into()),
      icon: Icon::App(vec![
        n.app_icon.clone().into(),
        n.desktop_entry.clone().unwrap_or_default().into(),
        n.app_name.clone().into(),
      ]),
      keywords: vec![n.app_name.clone().into(), body.into()],
      section: Section::Results,
      accessories: vec![Accessory::Text(when(n.time, "%H:%M").into())],
      ..Default::default()
    }
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let id = self.0.id;
    let invoke = |key: String, title: SharedString| {
      Action::sync(format!("action:{key}"), title, move |_, _, cx| {
        let notifications = cx.notifications().clone();
        notifications.invoke_action(id, &key, cx);
        Ok(Outcome::close())
      })
    };
    let mut actions = vec![invoke(
      "default".into(),
      t!("app.launcher.notifications.open").into(),
    )];
    for action in self.0.actions.iter().filter(|a| a.key != "default") {
      actions.push(invoke(action.key.clone(), action.label.clone().into()));
    }
    actions.push(
      Action::sync(
        "dismiss",
        t!("app.launcher.notifications.remove"),
        move |_, _, cx| {
          let notifications = cx.notifications().clone();
          notifications.dismiss(id, cx);
          Ok(Outcome::KeepOpen)
        },
      )
      .destructive()
      .keep_open(),
    );
    actions
  }

  fn detail(&self, _: &App) -> Option<Detail> {
    Some(Detail {
      body: DetailBody::Text(strip_markup(&self.0.body).into()),
      metadata: vec![
        (
          t!("app.launcher.notifications.app").into(),
          self.0.app_name.clone().into(),
        ),
        (
          t!("app.launcher.notifications.time").into(),
          when(self.0.time, "%d.%m. %H:%M").into(),
        ),
      ],
      tags: Vec::new(),
    })
  }
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      opener(
        "notification:search",
        t!("app.launcher.notifications.search_title"),
        Icon::Tile(IconName::BellRing, 280.),
        &["notifications", "history"],
        |_| {
          ListView::new(
            t!("app.launcher.notifications.search_title"),
            t!("app.launcher.notifications.search"),
            Rc::new(
              ListSource::new("notifications", |_, cx| {
                cx.notifications()
                  .list(cx)
                  .iter()
                  .rev()
                  .map(|n| Rc::new(NotificationRow(n.clone())) as Rc<dyn Command>)
                  .collect()
              })
              .observe(|notify, cx| {
                let list = cx.notifications().notifications.clone();
                vec![watch(&list, &notify, cx)]
              }),
            ),
          )
          .preview()
          .empty(t!("app.launcher.notifications.none"))
          .into_view()
        },
      ),
      simple(
        "notification:dismiss",
        |_| Meta {
          title: t!("app.launcher.notifications.dismiss").into(),
          icon: Icon::Tile(IconName::BellOff, 280.),
          keywords: vec!["notifications".into(), "toasts".into(), "popups".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "dismiss",
            t!("app.launcher.notifications.dismiss"),
            |_, _, cx| {
              DismissToasts::handle((), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      ),
    ],
    cx,
  );
}
