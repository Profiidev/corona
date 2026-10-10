use std::{mem, pin::pin, time::UNIX_EPOCH};

use anyhow::anyhow;
use corona_notifications as nt;
use corona_notifications::NotificationsExt;
use futures_lite::StreamExt;
use gpui_kit::{App, BorrowAppContext};
use gpui_shell::HostModule;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  ScriptManager,
  host_fn::{Glob, Module},
  module::{PluginRef, Subscribe, Subscriptions, plugin::ActionEvent, read},
};
use corona_macros::named;

#[derive(Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum Urgency {
  Low,
  Normal,
  Critical,
}

#[derive(Serialize, Deserialize, TS)]
struct Action {
  key: String,
  label: String,
}

impl From<Urgency> for nt::Urgency {
  fn from(value: Urgency) -> Self {
    match value {
      Urgency::Low => nt::Urgency::Low,
      Urgency::Normal => nt::Urgency::Normal,
      Urgency::Critical => nt::Urgency::Critical,
    }
  }
}

/// A notification from the plugin, under its name.
#[derive(Serialize, Deserialize, TS)]
struct NotifyOptions {
  summary: String,
  #[ts(optional)]
  body: Option<String>,
  /// A theme icon name or a `file://` path.
  #[ts(optional)]
  icon: Option<String>,
  /// Buttons; the one with key `default` is a click on the notification.
  #[ts(optional)]
  actions: Option<Vec<Action>>,
  #[ts(optional)]
  urgency: Option<Urgency>,
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

/// Hands the actions picked on plugin notifications to the plugin that sent
/// them, from the first `notify` on
fn listen(cx: &mut App) {
  let manager = cx.global_mut::<ScriptManager>();
  if mem::replace(&mut manager.listening, true) {
    return;
  }
  let owners = manager.owners.clone();
  let actions = cx.notifications().action_invoked();
  cx.spawn(async move |cx| {
    let actions = match actions.await {
      Ok(actions) => actions,
      Err(e) => return tracing::error!("notification actions: {e:#}"),
    };
    let mut actions = pin!(actions);
    while let Some((id, key)) = actions.next().await {
      // ponytail: forgotten after the first action, a resident notification
      // delivers only that one; keep owners until NotificationClosed if needed
      let Some(owner) = owners.lock().unwrap().remove(&id) else {
        continue;
      };
      cx.update(|cx| {
        if let Some(hub) = cx.global::<ScriptManager>().hubs.get(&owner) {
          hub.push_action(ActionEvent { id, key });
        }
      });
    }
  })
  .detach();
}

pub fn module(
  plugin: PluginRef,
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  cx: &mut App,
) -> HostModule {
  let hub = cx.update_global::<ScriptManager, _>(|manager, cx| manager.hub(plugin.id, cx));
  let load = hub.load();
  let stop = hub.clone();
  subs.push(Subscribe::Cleanup(gpui_kit::Subscription::new(move || {
    stop.stop_actions(load)
  })));
  let (id, name) = (plugin.id.to_string(), plugin.name.to_string());
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
    .func(named!(
      "notify",
      /// Shows a notification under the plugin's name, its id.
      move |cx: &mut App, options: NotifyOptions| {
        listen(cx);
        let owners = cx.global::<ScriptManager>().owners.clone();
        let sent = cx.notifications().send_notify(nt::Notify {
          app_name: name.clone(),
          app_icon: options.icon.unwrap_or_default(),
          summary: options.summary,
          body: options.body.unwrap_or_default(),
          actions: (options.actions.unwrap_or_default().into_iter())
            .map(|a| nt::Action {
              key: a.key,
              label: a.label,
            })
            .collect(),
          urgency: options.urgency.unwrap_or(Urgency::Normal).into(),
        });
        let id = id.clone();
        async move {
          let sent = sent.await?;
          owners.lock().unwrap().insert(sent, id);
          anyhow::Ok(sent)
        }
      }
    ))
    .func(named!(
      "nextAction",
      /// The next action picked on one of the plugin's notifications, once
      /// one is. Every view waiting gets it.
      move || {
        let rx = hub.next_action(load);
        async move { rx.recv_async().await.map_err(|_| anyhow!("plugin stopped")) }
      }
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

#[cfg(test)]
mod tests {
  use std::time::{Duration, SystemTime};

  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use gpui_shell::ShellRuntime;

  use super::*;
  use crate::{module::harness, plugin::paths::Paths};

  fn notification(urgency: nt::Urgency, time: SystemTime) -> nt::Notification {
    nt::Notification {
      id: 4,
      app_name: "mail".into(),
      app_icon: String::new(),
      summary: "New mail".into(),
      body: "Hi".into(),
      actions: vec![nt::Action {
        key: "default".into(),
        label: "Open".into(),
      }],
      urgency,
      desktop_entry: Some("thunderbird".into()),
      resident: true,
      time,
      read: false,
    }
  }

  #[test]
  fn converts() {
    let all = [
      (nt::Urgency::Low, "low"),
      (nt::Urgency::Normal, "normal"),
      (nt::Urgency::Critical, "critical"),
    ];
    let time = UNIX_EPOCH + Duration::from_millis(1500);
    for (urgency, name) in all {
      let json = serde_json::to_value(Notification::from(&notification(urgency, time))).unwrap();
      assert_eq!(json["urgency"], name);
      assert_eq!(json["time"], 1.5);
      assert_eq!(json["actions"][0]["key"], "default");
      assert_eq!(json["actions"][0]["label"], "Open");
      assert_eq!(json["desktop_entry"], "thunderbird");
      assert_eq!(json["read"], false);
      assert!(json.get("resident").is_none());
    }
  }

  #[test]
  fn time_before_the_epoch_is_zero() {
    let time = UNIX_EPOCH - Duration::from_secs(1);
    let converted = Notification::from(&notification(nt::Urgency::Normal, time));
    assert_eq!(converted.time, 0.);
  }

  #[gpui::test]
  fn actions_reach_the_plugin_that_notified(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      let executor = cx.foreground_executor().clone();
      executor.block_on(nt::init(cx, &conn, true)).unwrap()
    });
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: dir.path().join("state"),
      local: dir.path().join("local"),
    };
    cx.set_global(ScriptManager::new(
      ShellRuntime::new_isolated().unwrap(),
      paths,
    ));

    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      m.notify({ summary: "hi", actions: [{ key: "open", label: "Open" }] }).then((id) => {
        report(id);
        m.nextAction().then(report);
      });
    }"#;
    let plugin = PluginRef {
      id: "a",
      name: "Plugin A",
    };
    let (view, cx) = harness::view(cx, body, |reads, subs, cx| module(plugin, reads, subs, cx));
    wait_until(cx, |cx| {
      !view.reports.borrow().is_empty() && cx.read(|cx| !cx.notifications().list(cx).is_empty())
    });
    let id = view.reports.borrow()[0].as_u64().unwrap() as u32;
    let app_name = cx.read(|cx| cx.notifications().list(cx)[0].app_name.clone());
    assert_eq!(app_name, "Plugin A");

    cx.update(|_, cx| cx.notifications().clone().invoke_action(id, "open", cx));
    wait_until(cx, |_| view.reports.borrow().len() == 2);
    assert_eq!(view.last(), serde_json::json!({ "id": id, "key": "open" }));
  }
}
