//! What the launcher asks of the shell it runs in

use std::rc::Rc;

use gpui_kit::{App, Global};

type OpenSettings = Rc<dyn Fn(Option<&str>, &mut App)>;
type Notify = Rc<dyn Fn(String, String, &mut App)>;

#[derive(Default)]
struct Hooks {
  open_settings: Option<OpenSettings>,
  notify: Option<Notify>,
}

impl Global for Hooks {}

/// Opens the settings window, on a page when given
pub fn set_open_settings(open: impl Fn(Option<&str>, &mut App) + 'static, cx: &mut App) {
  cx.default_global::<Hooks>().open_settings = Some(Rc::new(open));
}

pub fn open_settings(page: Option<&str>, cx: &mut App) {
  if let Some(open) = cx
    .try_global::<Hooks>()
    .and_then(|h| h.open_settings.clone())
  {
    open(page, cx);
  }
}

/// Sends a desktop notification, like a finished timer
pub fn set_notify(notify: impl Fn(String, String, &mut App) + 'static, cx: &mut App) {
  cx.default_global::<Hooks>().notify = Some(Rc::new(notify));
}

pub fn notify(summary: String, body: String, cx: &mut App) {
  if let Some(notify) = cx.try_global::<Hooks>().and_then(|h| h.notify.clone()) {
    notify(summary, body, cx);
  }
}
