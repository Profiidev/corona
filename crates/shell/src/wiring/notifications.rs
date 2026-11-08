//! Sends the notifications of the plugin host and the launcher

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use corona_utils::error::spawn_logged;

use corona_notifications::{Action, Notifications, NotificationsExt, Notify, Urgency};
use corona_launcher::hooks::LauncherEvent;
use corona_script::hooks::{self, Click, Notice, ScriptEvent};
use futures_lite::StreamExt;
use gpui_kit::App;

/// Clicking the notification, no button
const CLICK: &str = "default";

/// What clicking each notification sent does, by its id
type Clicks = Rc<RefCell<HashMap<u32, Click>>>;

pub(super) fn init(cx: &mut App) {
  let clicks = Clicks::default();
  listen(clicks.clone(), cx);
  let script = hooks::events(cx);
  cx.subscribe(&script, move |_, ScriptEvent::Notify(notice), cx| {
    send(notice.clone(), clicks.clone(), cx)
  })
  .detach();
  let launcher = corona_launcher::hooks::events(cx);
  cx.subscribe(&launcher, |_, LauncherEvent::Notify { summary, body }, cx| {
    let send = cx.notifications().send(summary.clone(), body.clone());
    spawn_logged(cx, send);
  })
  .detach();
}

fn send(notice: Notice, clicks: Clicks, cx: &mut App) {
  let Some(notifications) = cx.try_global::<Notifications>() else {
    return;
  };
  let (actions, click) = match notice.action {
    Some((label, click)) => (
      vec![Action {
        key: CLICK.into(),
        label,
      }],
      Some(click),
    ),
    None => (Vec::new(), None),
  };
  let send = notifications.send_notify(Notify {
    app_name: corona_config::APP_NAME.into(),
    app_icon: String::new(),
    summary: notice.summary,
    body: notice.body,
    actions,
    urgency: Urgency::Normal,
    hints: HashMap::new(),
    // the default popup time, then it waits in the notification center
    timeout: -1,
  });
  cx.spawn(async move |_| match send.await {
    Ok(id) => {
      if let Some(click) = click {
        clicks.borrow_mut().insert(id, click);
      }
    }
    Err(e) => tracing::error!("plugin notification: {e:#}"),
  })
  .detach();
}

/// Runs what clicking a sent notification does
fn listen(clicks: Clicks, cx: &mut App) {
  let Some(actions) = cx.try_global::<Notifications>().map(|n| n.action_invoked()) else {
    return;
  };
  cx.spawn(async move |cx| {
    let mut actions = match actions.await {
      Ok(actions) => std::pin::pin!(actions),
      Err(e) => return tracing::error!("plugin notification actions: {e:#}"),
    };
    while let Some((notification, _)) = actions.next().await {
      let click = clicks.borrow_mut().remove(&notification);
      if let Some(click) = click {
        cx.update(|cx| click(cx));
      }
    }
  })
  .detach();
}
