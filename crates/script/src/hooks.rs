//! What the plugin host tells the shell it runs in

use std::rc::Rc;

use gpui_kit::{App, AppContext, Entity, EventEmitter, Global};

/// What clicking a notification does
pub type Click = Rc<dyn Fn(&mut App)>;

/// A desktop notification of the plugin host
#[derive(Clone)]
pub struct Notice {
  pub summary: String,
  pub body: String,
  /// A button's label and what clicking the notification or it does
  pub action: Option<(String, Click)>,
}

/// Plugin updates and plugins waiting for approval ask to be notified about
#[derive(Clone)]
pub enum ScriptEvent {
  Notify(Notice),
}

/// Emits the [`ScriptEvent`]s
pub struct ScriptEvents;

impl EventEmitter<ScriptEvent> for ScriptEvents {}

struct Events(Entity<ScriptEvents>);

impl Global for Events {}

/// Subscribe to send the notifications of plugin updates and of plugins waiting for approval
pub fn events(cx: &mut App) -> Entity<ScriptEvents> {
  if let Some(events) = cx.try_global::<Events>() {
    return events.0.clone();
  }
  let events = cx.new(|_| ScriptEvents);
  cx.set_global(Events(events.clone()));
  events
}

pub(crate) fn notify(notice: Notice, cx: &mut App) {
  events(cx).update(cx, |_, cx| cx.emit(ScriptEvent::Notify(notice)));
}
