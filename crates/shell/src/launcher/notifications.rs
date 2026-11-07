//! Searching the notification history, dismissing toasts, do not disturb, clearing

use std::rc::Rc;

use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, Command, Detail, DetailBody, Icon, ListView, Meta, Outcome, Section,
  kit::{Group, ListSource, Toggle, opener, watch},
  registry, simple,
};
use corona_surface::{commands::OpenPage, control_center::ControlCenterPage};

use crate::panel::notifications::NotificationsPanel;
use corona_notifications::{Notification, NotificationsExt, strip_markup};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use corona_components::i18n;

use crate::ipc::notifications::{ClearHistory, DismissToasts, Dnd, DoNotDisturb};

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
    let mut actions = Vec::new();
    if self.0.actions.iter().any(|a| a.key == "default") {
      actions.push(invoke(
        "default".into(),
        t!("app.launcher.notifications.open").into(),
      ));
    }
    for action in self
      .0
      .actions
      .iter()
      .filter(|a| a.key != "default" && a.key != "inline-reply")
    {
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

pub(crate) fn register(cx: &mut App) {
  dnd(cx);
  tools(cx);
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

/// Do not disturb as a toggle
fn dnd(cx: &mut App) {
  let toggle = Toggle::command(
    "dnd",
    t!("app.launcher.system.dnd"),
    IconName::BellOff,
    280.,
    |cx| Some(cx.notifications().do_not_disturb(cx)),
    |on, cx| {
      let dnd = match on {
        true => Dnd::On,
        false => Dnd::Off,
      };
      DoNotDisturb::handle(dnd, cx).map(|_| ())
    },
  );
  Group::new("system:dnd", vec![toggle])
    .observe(|notify, cx| {
      let notifications = cx.notifications().clone();
      vec![watch(&notifications.do_not_disturb, &notify, cx)]
    })
    .register(cx);
}

/// Opening the notifications page and clearing the history
fn tools(cx: &mut App) {
  let commands: Vec<Rc<dyn Command>> = vec![
    simple(
      "notification:show",
      |_| Meta {
        title: t!("app.launcher.tools.show_notifications").into(),
        icon: Icon::Tile(IconName::Bell, 280.),
        keywords: vec!["notification center".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync("open", t!("app.launcher.open"), |_, _, cx| {
          OpenPage::handle(NotificationsPanel::ID.into(), cx)?;
          Ok(Outcome::close())
        })]
      },
    ),
    simple(
      "notification:clear_history",
      |cx| Meta {
        title: t!("app.launcher.tools.clear_notifications").into(),
        subtitle: Some(
          t!(
            "app.launcher.tools.notification_count",
            count = cx.notifications().list(cx).len()
          )
          .into(),
        ),
        icon: Icon::Tile(IconName::BellMinus, 280.),
        keywords: vec!["notifications".into()],
        ..Default::default()
      },
      |_| {
        vec![
          Action::sync(
            "clear",
            t!("app.launcher.tools.clear_notifications"),
            |_, _, cx| {
              ClearHistory::handle((), cx)?;
              Ok(Outcome::hud(t!("app.launcher.tools.cleared")))
            },
          )
          .destructive(),
        ]
      },
    ),
  ];
  registry::register_all(commands, cx);
}
