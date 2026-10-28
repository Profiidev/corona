//! What the launcher asks of the shell it runs in

use std::rc::Rc;

use gpui_kit::{App, Global};

type Notify = Rc<dyn Fn(String, String, &mut App)>;

#[derive(Default)]
struct Hooks {
  notify: Option<Notify>,
}

impl Global for Hooks {}

/// Sends a desktop notification, like a finished timer
pub fn set_notify(notify: impl Fn(String, String, &mut App) + 'static, cx: &mut App) {
  cx.default_global::<Hooks>().notify = Some(Rc::new(notify));
}

pub fn notify(summary: String, body: String, cx: &mut App) {
  if let Some(notify) = cx.try_global::<Hooks>().and_then(|h| h.notify.clone()) {
    notify(summary, body, cx);
  }
}
