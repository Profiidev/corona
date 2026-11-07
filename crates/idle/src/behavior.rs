//! The `[idle]` behaviors: lock, monitors off, suspend or a command after a
//! while without input, like hypridle.

use std::{cell::RefCell, collections::HashMap, process::Command, time::Duration};

use crate::{Idle, IdleExt};
use corona_compositor::CompositorExt;
use corona_config::{ConfigProvider, IdleAction, IdleConfig, observe_section};
use corona_power::PowerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, BorrowAppContext, Global};


pub(crate) fn init(cx: &mut App, session: &zbus::Connection) {
  crate::start(cx, session, changed);
  watch(cx);
  observe_section(cx, |c| &c.idle, |_, cx| watch(cx));
  // logind's own idle action and sleep wait too
  let stay_awake = cx.idle().stay_awake.clone();
  cx.power().clone().stay_awake_while(&stay_awake, cx);
}

fn watch(cx: &mut App) {
  let timeouts = timeouts(&cx.config().idle);
  cx.update_global::<Idle, _>(|idle, cx| idle.set_timeouts(timeouts, cx));
}

/// Each behavior that runs at all, with its timeout
fn timeouts(config: &IdleConfig) -> Vec<(String, Duration)> {
  config
    .behavior
    .iter()
    .filter_map(|(name, behavior)| Some((name.clone(), behavior.after()?)))
    .collect()
}

thread_local! {
  /// What each idle behavior did, so resuming undoes that even if the config changed meanwhile
  static FIRED: RefCell<HashMap<String, (IdleAction, String)>> = RefCell::default();
}

fn changed(name: &str, idle: bool, cx: &mut App) {
  tracing::debug!("idle behavior {name}: idle {idle}");
  if idle {
    let Some(behavior) = cx.config().idle.behavior.get(name).cloned() else {
      return;
    };
    FIRED.with_borrow_mut(|f| {
      f.insert(
        name.to_string(),
        (behavior.action, behavior.resume_command.clone()),
      )
    });
    match behavior.action {
      IdleAction::Lock
      | IdleAction::Suspend
      | IdleAction::LockAndSuspend
      | IdleAction::LockAndSuspendThenHibernate => session(behavior.action, cx),
      IdleAction::ScreenOff => {
        let _ = cx.compositor().set_dpms(false).log_err();
      }
      IdleAction::Command => run(&behavior.command),
    }
  } else if let Some((action, resume_command)) = FIRED.with_borrow_mut(|f| f.remove(name)) {
    match action {
      IdleAction::ScreenOff => {
        let _ = cx.compositor().set_dpms(true).log_err();
      }
      IdleAction::Command => run(&resume_command),
      _ => {}
    }
  }
}

/// Runs the lock or power action of a behavior; the shell sets it, nothing here knows how
pub struct SessionHook(pub Rc<dyn Fn(IdleAction, &mut App)>);

impl Global for SessionHook {}

fn session(action: IdleAction, cx: &mut App) {
  match cx.try_global::<SessionHook>().map(|h| h.0.clone()) {
    Some(run) => run(action, cx),
    None => tracing::warn!("idle {action:?}: nothing runs session actions"),
  }
}

/// Runs `command` in a shell, waited on in its own thread so it leaves no zombie
fn run(command: &str) {
  if command.trim().is_empty() {
    return;
  }
  let child = Command::new("sh").arg("-c").arg(command).spawn();
  match child {
    Ok(mut child) => {
      std::thread::spawn(move || child.wait());
    }
    Err(e) => tracing::error!("idle command `{command}` did not start: {e}"),
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use corona_config::{Config, IdleAction, IdleBehavior, IdleConfig};
  use gpui_kit::{self as gpui, TestAppContext};

  use corona_surface::testing::{FakeCompositor, setup};

  #[test]
  fn timeouts() {
    let mut defaults = super::timeouts(&IdleConfig::default());
    defaults.sort();
    let secs = |s| Duration::from_secs(s);
    assert_eq!(
      defaults,
      [
        ("lock".to_string(), secs(600)),
        ("lock-and-suspend".to_string(), secs(900)),
        ("screen-off".to_string(), secs(660)),
      ]
    );

    let mut config = IdleConfig::default();
    config.behavior.clear();
    let behavior = |timeout| IdleBehavior {
      timeout,
      ..Default::default()
    };
    let off = IdleBehavior {
      enabled: false,
      ..behavior(30.)
    };
    config.behavior.insert("off".into(), off);
    config.behavior.insert("custom".into(), behavior(90.5));
    config.behavior.insert("never".into(), behavior(0.));
    config.behavior.insert("broken".into(), behavior(-1.));
    config.behavior.insert("nan".into(), behavior(f64::NAN));
    let mut timeouts = super::timeouts(&config);
    timeouts.sort();
    assert_eq!(
      timeouts,
      [("custom".to_string(), Duration::from_millis(90_500)),]
    );
  }

  #[gpui::test]
  fn screen_off_and_back(cx: &mut TestAppContext) {
    let fake = setup(FakeCompositor::default(), cx);
    cx.update(|cx| {
      let mut config = Config::default();
      config.idle.behavior.clear();
      let behavior = |action| IdleBehavior {
        action,
        ..Default::default()
      };
      config
        .idle
        .behavior
        .insert("off".into(), behavior(IdleAction::ScreenOff));
      // no command set, so nothing runs
      config
        .idle
        .behavior
        .insert("cmd".into(), behavior(IdleAction::Command));
      cx.set_global(config);

      super::changed("off", true, cx);
      super::changed("off", false, cx);
      super::changed("cmd", true, cx);
      super::changed("cmd", false, cx);
      super::changed("unknown", true, cx);
    });
    assert_eq!(*fake.calls.borrow(), ["dpms false", "dpms true"]);
  }
}
