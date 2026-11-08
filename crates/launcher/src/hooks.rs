//! What the launcher tells the shell it runs in

use gpui_kit::{App, AppContext, Entity, EventEmitter, Global};

#[derive(Clone, Debug, PartialEq)]
pub enum LauncherEvent {
  /// A desktop notification is due, like a finished timer
  Notify { summary: String, body: String },
}

/// Emits the [`LauncherEvent`]s
pub struct LauncherEvents;

impl EventEmitter<LauncherEvent> for LauncherEvents {}

struct Events(Entity<LauncherEvents>);

impl Global for Events {}

/// Subscribe to follow what the launcher asks of the shell
pub fn events(cx: &mut App) -> Entity<LauncherEvents> {
  if let Some(events) = cx.try_global::<Events>() {
    return events.0.clone();
  }
  let events = cx.new(|_| LauncherEvents);
  cx.set_global(Events(events.clone()));
  events
}

pub fn notify(summary: String, body: String, cx: &mut App) {
  events(cx).update(cx, |_, cx| cx.emit(LauncherEvent::Notify { summary, body }));
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, rc::Rc};

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  #[gpui::test]
  fn notify_reaches_subscribers(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
      let seen = seen.clone();
      let events = events(cx);
      cx.subscribe(&events, move |_, event: &LauncherEvent, _| {
        seen.borrow_mut().push(event.clone())
      })
      .detach();
      notify("Timer".into(), "done".into(), cx);
    });
    assert_eq!(
      *seen.borrow(),
      [LauncherEvent::Notify {
        summary: "Timer".into(),
        body: "done".into()
      }]
    );
  }
}
