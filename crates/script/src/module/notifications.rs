use std::time::UNIX_EPOCH;

use corona_notifications as nt;
use corona_notifications::NotificationsExt;
use gpui_kit::App;
use gpui_shell::HostModule;
use serde::Serialize;
use ts_rs::TS;

use crate::{
  host_fn::{Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Urgency {
  Low,
  Normal,
  Critical,
}

#[derive(Serialize, TS)]
struct Action {
  key: String,
  label: String,
}

#[derive(Serialize, TS)]
struct Notification {
  id: u32,
  app_name: String,
  /// A theme icon name or a `file://` path, empty when the app sent none.
  app_icon: String,
  summary: String,
  body: String,
  actions: Vec<Action>,
  urgency: Urgency,
  desktop_entry: Option<String>,
  /// Unix time in seconds.
  time: f64,
  /// Seen in the notification panel.
  read: bool,
}

impl From<&nt::Notification> for Notification {
  fn from(n: &nt::Notification) -> Self {
    Self {
      id: n.id,
      app_name: n.app_name.clone(),
      app_icon: n.app_icon.clone(),
      summary: n.summary.clone(),
      body: n.body.clone(),
      actions: n
        .actions
        .iter()
        .map(|a| Action {
          key: a.key.clone(),
          label: a.label.clone(),
        })
        .collect(),
      urgency: match n.urgency {
        nt::Urgency::Low => Urgency::Low,
        nt::Urgency::Normal => Urgency::Normal,
        nt::Urgency::Critical => Urgency::Critical,
      },
      desktop_entry: n.desktop_entry.clone(),
      time: n
        .time
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64()),
      read: n.read,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Notifications,
  DoNotDisturb,
  Active,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Notifications(value)
  }
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  let state = cx.notifications();

  Module::new("corona/notifications")
    .func(read(
      reads,
      subs,
      "listNotifications",
      Updates::Notifications,
      state.notifications.clone(),
      |cx| {
        let list = cx.notifications().list(cx);
        list.iter().map(Notification::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "hasUnread",
      Updates::Notifications,
      state.notifications.clone(),
      |cx| cx.notifications().has_unread(cx),
    ))
    .func(read(
      reads,
      subs,
      "doNotDisturb",
      Updates::DoNotDisturb,
      state.do_not_disturb.clone(),
      |cx| cx.notifications().do_not_disturb(cx),
    ))
    .func(read(
      reads,
      subs,
      "active",
      Updates::Active,
      state.active.clone(),
      // false while another notification daemon owns the bus name
      |cx| cx.notifications().active(cx),
    ))
    .func(named!("setDoNotDisturb", |cx: &mut App, enabled: bool| cx
      .notifications()
      .clone()
      .set_do_not_disturb(enabled, cx)))
    .func(named!("dismiss", |cx: &mut App, id: u32| cx
      .notifications()
      .clone()
      .dismiss(id, cx)))
    .func(named!("markRead", |cx: &mut App, id: u32| cx
      .notifications()
      .clone()
      .mark_read(id, cx)))
    .func(named!("markAllRead", |cx: &mut App| cx
      .notifications()
      .clone()
      .mark_all_read(cx)))
    .func(named!(
      "send",
      /// Shows a notification from corona.
      |notifications: Glob<nt::Notifications>, summary: String, body: String| notifications
        .send(summary, body)
    ))
    .func(named!("clearAll", |cx: &mut App| cx
      .notifications()
      .clone()
      .clear_all(cx)))
    .func(named!(
      "invokeAction",
      /// Tells the app, then closes the notification unless it is resident.
      |cx: &mut App, id: u32, key: String| cx.notifications().clone().invoke_action(id, &key, cx)
    ))
    .into()
}
