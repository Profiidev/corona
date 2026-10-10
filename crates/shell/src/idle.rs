//! The `[idle]` behaviors: lock, monitors off, suspend or a command after a
//! while without input, like hypridle.

use std::{process::Command, time::Duration};

use corona_compositor::CompositorExt;
use corona_config::{ConfigProvider, IdleAction, IdleConfig, observe_section};
use corona_idle::IdleExt;
use corona_ipc::IpcCommand;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

use crate::commands::session::{Action, Session};

pub fn init(cx: &mut App, session: &zbus::Connection) {
  corona_idle::init(cx, session, changed);
  watch(cx);
  observe_section(cx, |c| &c.idle, |_, cx| watch(cx));
}

fn watch(cx: &mut App) {
  let timeouts = timeouts(&cx.config().idle);
  cx.idle().set_timeouts(timeouts);
}

/// Each behavior that runs at all, with its timeout
fn timeouts(config: &IdleConfig) -> Vec<(String, Duration)> {
  config
    .behavior
    .iter()
    .filter_map(|(name, behavior)| Some((name.clone(), behavior.after()?)))
    .collect()
}

fn changed(name: &str, idle: bool, cx: &mut App) {
  let Some(behavior) = cx.config().idle.behavior.get(name).cloned() else {
    return;
  };
  tracing::debug!("idle behavior {name}: idle {idle}");
  if idle {
    match behavior.action {
      IdleAction::Lock => session(Action::Lock, cx),
      IdleAction::ScreenOff => {
        let _ = cx.compositor().set_dpms(false).log_err();
      }
      IdleAction::Suspend => session(Action::Suspend, cx),
      IdleAction::LockAndSuspend => session(Action::LockAndSuspend, cx),
      IdleAction::LockAndSuspendThenHibernate => session(Action::LockAndSuspendThenHibernate, cx),
      IdleAction::Command => run(&behavior.command),
    }
  } else {
    match behavior.action {
      IdleAction::ScreenOff => {
        let _ = cx.compositor().set_dpms(true).log_err();
      }
      IdleAction::Command => run(&behavior.resume_command),
      _ => {}
    }
  }
}

fn session(action: Action, cx: &mut App) {
  let _ = Session::handle(action, cx).log_err();
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

  use crate::test_support::{FakeCompositor, setup};

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
