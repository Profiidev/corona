//! Runs the plugins, and sends the plugin host's notifications until the notifications
//! integration takes them over

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use corona_notifications::{Action, Notifications, Notify, Urgency};
use corona_script::hooks::{self, Click, Notice};
use futures::StreamExt;
use gpui_kit::App;

/// Clicking the notification, no button
const CLICK: &str = "default";

/// What clicking each notification sent does, by its id
type Clicks = Rc<RefCell<HashMap<u32, Click>>>;

pub fn init(cx: &mut App) {
  let clicks = Clicks::default();
  listen(clicks.clone(), cx);
  hooks::set_notify(move |notice, cx| send(notice, clicks.clone(), cx), cx);
  corona_script::init(cx).expect("Failed to init plugins");
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
        cx.update(click);
      }
    }
  })
  .detach();
}
