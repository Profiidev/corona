use std::{collections::HashMap, io::Cursor, mem, pin::pin, time::UNIX_EPOCH};

use anyhow::{anyhow, bail, ensure};
use corona_notifications as nt;
use corona_notifications::NotificationsExt;
use futures_lite::{StreamExt, future::try_zip, stream};
use gpui_kit::{App, BorrowAppContext};
use gpui_shell::HostModule;
use image::{ImageError, ImageReader, Limits};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use ts_rs::TS;
use zbus::zvariant::{OwnedValue, Value};

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

/// The action key of a typed reply
const REPLY: &str = "inline-reply";
/// Hints set by options, and `desktop-entry` as the plugin shows under its own name
const RESERVED_HINTS: [&str; 8] = [
  "urgency",
  "image-data",
  "image_data",
  "icon_data",
  "image-path",
  "image_path",
  "x-kde-reply-placeholder-text",
  "desktop-entry",
];
// ponytail: bigger pictures are rejected, scale them down here if plugins need them
const MAX_IMAGE_SIDE: u32 = 1024;

/// A spec hint: whole numbers go out as `int32`, others as `double`
#[derive(Deserialize, TS)]
#[serde(untagged)]
enum Hint {
  Bool(bool),
  Number(f64),
  Text(String),
}

#[derive(Deserialize, TS)]
struct Reply {
  #[ts(optional)]
  placeholder: Option<String>,
}

/// A notification from the plugin, under its name.
#[derive(Deserialize, TS)]
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
  /// A picture, PNG or JPEG, at most 1024 pixels a side.
  #[ts(optional, type = "Uint8Array")]
  image: Option<Bytes>,
  /// More spec hints, like `resident` or `category`. The ones options set
  /// (`urgency`, the image ones, the reply placeholder) and `desktop-entry`
  /// are rejected.
  #[ts(optional)]
  hints: Option<HashMap<String, Hint>>,
  /// Milliseconds on screen: -1 the default, 0 until dismissed.
  #[ts(optional)]
  timeout: Option<i32>,
  /// A reply field; the text arrives at `nextAction` with key `inline-reply`.
  #[ts(optional)]
  reply: Option<Reply>,
}

/// A `Uint8Array`'s bytes
struct Bytes(Vec<u8>);

// ponytail: a Uint8Array crosses the host bridge as an object of its bytes by
// index, a bytes HostValue in gpui-shell if big pictures get slow
impl<'de> Deserialize<'de> for Bytes {
  fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
    let indexed = HashMap::<usize, u8>::deserialize(d)?;
    let mut bytes = vec![0; indexed.len()];
    for (i, byte) in indexed {
      *bytes
        .get_mut(i)
        .ok_or_else(|| D::Error::custom("not a Uint8Array"))? = byte;
    }
    Ok(Self(bytes))
  }
}

/// `(iiibiiay)` RGBA pixels for the `image-data` hint
fn image_data(encoded: &[u8]) -> anyhow::Result<OwnedValue> {
  let mut reader = ImageReader::new(Cursor::new(encoded)).with_guessed_format()?;
  let mut limits = Limits::default();
  limits.max_image_width = Some(MAX_IMAGE_SIDE);
  limits.max_image_height = Some(MAX_IMAGE_SIDE);
  reader.limits(limits);
  let image = match reader.decode() {
    Ok(image) => image.into_rgba8(),
    Err(ImageError::Limits(_)) => bail!("image: over {MAX_IMAGE_SIDE} pixels a side"),
    Err(e) => bail!("image: {e}"),
  };
  let (width, height) = (image.width() as i32, image.height() as i32);
  let data = (width, height, width * 4, true, 8, 4, image.into_raw());
  Ok(Value::from(data).try_to_owned()?)
}

impl NotifyOptions {
  fn into_notify(self, app_name: String) -> anyhow::Result<nt::Notify> {
    let mut hints = HashMap::new();
    for (key, hint) in self.hints.unwrap_or_default() {
      ensure!(
        !RESERVED_HINTS.contains(&key.as_str()),
        "hint {key} is reserved"
      );
      let value = match hint {
        Hint::Bool(b) => Value::from(b),
        Hint::Number(n) if n.fract() == 0. && n >= i32::MIN.into() && n <= i32::MAX.into() => {
          Value::from(n as i32)
        }
        Hint::Number(n) => Value::from(n),
        Hint::Text(text) => Value::from(text),
      };
      hints.insert(key, value.try_to_owned()?);
    }
    if let Some(image) = self.image {
      hints.insert("image-data".into(), image_data(&image.0)?);
    }
    let mut actions: Vec<nt::Action> = (self.actions.unwrap_or_default().into_iter())
      .map(|a| nt::Action {
        key: a.key,
        label: a.label,
      })
      .collect();
    if let Some(reply) = self.reply {
      actions.push(nt::Action {
        key: REPLY.into(),
        label: "Reply".into(),
      });
      if let Some(placeholder) = reply.placeholder {
        let placeholder = Value::from(placeholder).try_to_owned()?;
        hints.insert("x-kde-reply-placeholder-text".into(), placeholder);
      }
    }
    let timeout = self.timeout.unwrap_or(-1);
    ensure!(
      timeout >= -1,
      "timeout {timeout}: -1 for the default, 0 for never or milliseconds"
    );
    Ok(nt::Notify {
      app_name,
      app_icon: self.icon.unwrap_or_default(),
      summary: self.summary,
      body: self.body.unwrap_or_default(),
      actions,
      urgency: self.urgency.unwrap_or(Urgency::Normal).into(),
      hints,
      timeout,
    })
  }
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
      body: nt::strip_markup(&n.body),
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
  let streams = try_zip(
    cx.notifications().action_invoked(),
    cx.notifications().replied(),
  );
  cx.spawn(async move |cx| {
    let (actions, replies) = match streams.await {
      Ok(streams) => streams,
      Err(e) => return tracing::error!("notification actions: {e:#}"),
    };
    let actions = actions.map(|(id, key)| ActionEvent {
      id,
      key,
      text: None,
    });
    let replies = replies.map(|(id, text)| ActionEvent {
      id,
      key: REPLY.into(),
      text: Some(text),
    });
    let mut events = pin!(stream::or(actions, replies));
    while let Some(event) = events.next().await {
      let owner = {
        let mut owners = owners.lock().unwrap();
        match owners.get(&event.id) {
          // ponytail: kept until restart, drop on NotificationClosed if
          // plugins send many resident ones
          Some((owner, true)) => Some(owner.clone()),
          Some(_) => owners.remove(&event.id).map(|(owner, _)| owner),
          None => None,
        }
      };
      let Some(owner) = owner else {
        continue;
      };
      cx.update(|cx| {
        if let Some(hub) = cx.global::<ScriptManager>().hubs.get(&owner) {
          hub.push_action(event);
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
        let notify = options.into_notify(name.clone());
        let sent = notify.map(|n| {
          let resident = n.hints.get("resident").is_some_and(|v| **v == Value::Bool(true));
          (cx.notifications().send_notify(n), resident)
        });
        let id = id.clone();
        async move {
          let (sent, resident) = sent?;
          let sent = sent.await?;
          owners.lock().unwrap().insert(sent, (id, resident));
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
      image: None,
      summary: "New mail".into(),
      body: "Hi".into(),
      actions: vec![nt::Action {
        key: "default".into(),
        label: "Open".into(),
      }],
      urgency,
      desktop_entry: Some("thunderbird".into()),
      reply_placeholder: None,
      expire_timeout: -1,
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

    let body = format!(
      r#"if (!globalThis.started) {{
      globalThis.started = true;
      m.notify({{
        summary: "hi",
        actions: [{{ key: "open", label: "Open" }}],
        hints: {{ resident: true }},
        reply: {{ placeholder: "Say hi" }},
        timeout: 0,
        image: new Uint8Array({:?}),
      }}).then((id) => {{
        report(id);
        m.nextAction().then((action) => {{
          report(action);
          m.nextAction().then(report);
        }});
      }});
    }}"#,
      png(2, 2)
    );
    let body = body.as_str();
    let capabilities = Default::default();
    let plugin = PluginRef {
      id: "a",
      name: "Plugin A",
      capabilities: &capabilities,
    };
    let (view, cx) = harness::view(cx, body, |reads, subs, cx| module(plugin, reads, subs, cx));
    wait_until(cx, |cx| {
      !view.reports.borrow().is_empty() && cx.read(|cx| !cx.notifications().list(cx).is_empty())
    });
    let id = view.reports.borrow()[0].as_u64().unwrap() as u32;
    let n = cx.read(|cx| cx.notifications().list(cx)[0].clone());
    assert_eq!(n.app_name, "Plugin A");
    assert_eq!(n.reply_placeholder.as_deref(), Some("Say hi"));
    assert_eq!((n.resident, n.expire_timeout), (true, 0));
    assert!(matches!(n.image, Some(nt::NotificationImage::Path(_))));

    cx.update(|_, cx| cx.notifications().clone().invoke_action(id, "open", cx));
    wait_until(cx, |_| view.reports.borrow().len() == 2);
    assert_eq!(view.last(), serde_json::json!({ "id": id, "key": "open" }));
    // resident, so the plugin still owns it
    cx.update(|_, cx| cx.notifications().clone().reply(id, "hey", cx));
    wait_until(cx, |_| view.reports.borrow().len() == 3);
    assert_eq!(
      view.last(),
      serde_json::json!({ "id": id, "key": "inline-reply", "text": "hey" })
    );
  }

  fn png(width: u32, height: u32) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::new(width, height)
      .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
      .unwrap();
    png
  }

  /// options as a script passes them, a Uint8Array as an object by index
  fn options(mut json: serde_json::Value) -> NotifyOptions {
    if let Some(bytes) = json.get("image").and_then(|i| i.as_array()) {
      let indexed: serde_json::Map<_, _> = (bytes.iter().enumerate())
        .map(|(i, b)| (i.to_string(), b.clone()))
        .collect();
      json["image"] = indexed.into();
    }
    serde_json::from_value(json).unwrap()
  }

  fn error(json: serde_json::Value) -> String {
    options(json)
      .into_notify("p".into())
      .err()
      .unwrap()
      .to_string()
  }

  #[test]
  fn options_become_hints() {
    let n = options(serde_json::json!({
      "summary": "s",
      "urgency": "critical",
      "hints": { "resident": true, "x": 5, "volume": 0.5, "category": "im" },
      "reply": {},
      "image": png(3, 2),
    }))
    .into_notify("p".into())
    .unwrap();
    assert_eq!((n.app_name.as_str(), n.timeout), ("p", -1));
    assert_eq!(n.hints["resident"], OwnedValue::from(true));
    assert_eq!(n.hints["x"], OwnedValue::from(5i32));
    assert_eq!(n.hints["volume"], OwnedValue::from(0.5));
    assert_eq!(&*n.hints["category"], &Value::from("im"));
    let (w, h, stride, alpha, bits, channels, pixels): (i32, i32, i32, bool, i32, i32, Vec<u8>) = n
      .hints["image-data"]
      .try_clone()
      .unwrap()
      .try_into()
      .unwrap();
    assert_eq!(
      (w, h, stride, alpha, bits, channels),
      (3, 2, 12, true, 8, 4)
    );
    assert_eq!(pixels.len(), 24);
    // a reply without placeholder is the action alone
    assert_eq!(n.actions[0].key, "inline-reply");
    assert!(!n.hints.contains_key("x-kde-reply-placeholder-text"));
  }

  #[test]
  fn bad_options_are_rejected() {
    for key in RESERVED_HINTS {
      let json = serde_json::json!({ "summary": "s", "hints": { key: 1 } });
      assert_eq!(error(json), format!("hint {key} is reserved"));
    }
    let json = serde_json::json!({ "summary": "s", "timeout": -2 });
    assert!(error(json).starts_with("timeout -2"));
    let json = serde_json::json!({ "summary": "s", "image": [1, 2, 3] });
    assert!(error(json).starts_with("image: "), "undecodable");
    let json = serde_json::json!({ "summary": "s", "image": png(MAX_IMAGE_SIDE + 1, 1) });
    assert_eq!(error(json), "image: over 1024 pixels a side");
    // holes are no Uint8Array
    let json = serde_json::json!({ "summary": "s", "image": { "0": 1, "2": 3 } });
    assert!(serde_json::from_value::<NotifyOptions>(json).is_err());
  }
}
