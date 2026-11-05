//! What the plugin host asks of the shell it runs in

use std::rc::Rc;

use gpui_kit::{App, Global};

/// What clicking a notification does
pub type Click = Box<dyn FnOnce(&mut App)>;

/// A desktop notification of the plugin host
pub struct Notice {
  pub summary: String,
  pub body: String,
  /// A button's label and what clicking the notification or it does
  pub action: Option<(String, Click)>,
}

type Notify = Rc<dyn Fn(Notice, &mut App)>;

#[derive(Default)]
struct Hooks {
  notify: Option<Notify>,
}

impl Global for Hooks {}

/// Sends the notifications of plugin updates and of plugins waiting for approval
pub fn set_notify(notify: impl Fn(Notice, &mut App) + 'static, cx: &mut App) {
  cx.default_global::<Hooks>().notify = Some(Rc::new(notify));
}

pub(crate) fn notify(notice: Notice, cx: &mut App) {
  if let Some(notify) = cx.try_global::<Hooks>().and_then(|h| h.notify.clone()) {
    notify(notice, cx);
  }
}
