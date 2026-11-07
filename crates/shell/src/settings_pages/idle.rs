//! The idle page: one group per behavior in the config

use corona_components::settings_fields::{choice, number, switch, text};
use corona_config::{Config, ConfigProvider, IdleAction, IdleBehavior};
use corona_settings::{SettingsExt, group, item, page, settings_page};
use gpui_kit::{
  App, Axis,
  assets::IconName,
  component::setting::{SettingGroup, SettingPage},
};
use rust_i18n::t;

settings_page!(IdleSettings, "idle", 110, |cx| idle(cx));

pub(super) fn register(cx: &mut App) {
  cx.settings_mut().register::<IdleSettings>();
}

/// The idle behavior `name`; a field set right after the behavior went from
/// the config writes to a new one
fn idle_behavior<'c>(c: &'c mut Config, name: &str) -> &'c mut IdleBehavior {
  c.idle.behavior.entry(name.to_string()).or_default()
}

/// Every setting of the idle behavior `name`
fn idle_group(name: String, action: IdleAction) -> SettingGroup {
  let read = {
    let name = name.clone();
    move |c: &Config| c.idle.behavior.get(&name).cloned().unwrap_or_default()
  };
  let write = |field: fn(&mut IdleBehavior, String)| {
    let name = name.clone();
    move |c: &mut Config, v: String| field(idle_behavior(c, &name), v)
  };
  let key = |field: &str| format!("idle.behavior.{name}.{field}");
  let actions = [
    (IdleAction::Lock, t!("app.settings.idle.action.lock")),
    (
      IdleAction::ScreenOff,
      t!("app.settings.idle.action.screen_off"),
    ),
    (IdleAction::Suspend, t!("app.settings.idle.action.suspend")),
    (
      IdleAction::LockAndSuspend,
      t!("app.settings.idle.action.lock_and_suspend"),
    ),
    (
      IdleAction::LockAndSuspendThenHibernate,
      t!("app.settings.idle.action.lock_and_suspend_then_hibernate"),
    ),
    (IdleAction::Command, t!("app.settings.idle.action.command")),
  ];
  // only the command action runs commands
  let commands = (action == IdleAction::Command).then(|| {
    [
      item(
        t!("app.settings.idle.command.title"),
        t!("app.settings.idle.command.description"),
        text(
          key("command"),
          {
            let read = read.clone();
            move |c| read(c).command
          },
          write(|b, v| b.command = v),
        ),
      )
      .layout(Axis::Vertical),
      item(
        t!("app.settings.idle.resume_command.title"),
        t!("app.settings.idle.resume_command.description"),
        text(
          key("resume_command"),
          {
            let read = read.clone();
            move |c| read(c).resume_command
          },
          write(|b, v| b.resume_command = v),
        ),
      )
      .layout(Axis::Vertical),
    ]
  });
  let mut items = vec![
    item(
      t!("app.settings.idle.enabled.title"),
      t!("app.settings.idle.enabled.description"),
      switch(
        {
          let read = read.clone();
          move |c| read(c).enabled
        },
        {
          let name = name.clone();
          move |c, v| idle_behavior(c, &name).enabled = v
        },
      ),
    ),
    item(
      t!("app.settings.idle.action.title"),
      t!("app.settings.idle.action.description"),
      choice(
        &actions,
        {
          let read = read.clone();
          move |c| read(c).action
        },
        {
          let name = name.clone();
          move |c, v| idle_behavior(c, &name).action = v
        },
      ),
    ),
    item(
      t!("app.settings.idle.after.title"),
      t!("app.settings.idle.after.description"),
      number(
        key("timeout"),
        (0., 86400., 30.),
        {
          let read = read.clone();
          move |c| read(c).timeout
        },
        {
          let name = name.clone();
          move |c, v| idle_behavior(c, &name).timeout = v
        },
      ),
    ),
  ];
  items.extend(commands.into_iter().flatten());
  group(name, items)
}

/// One group per idle behavior in the config
fn idle(cx: &App) -> SettingPage {
  page(
    t!("app.settings.idle.title"),
    IconName::Moon,
    cx.config()
      .idle
      .behavior
      .iter()
      .map(|(name, behavior)| idle_group(name.clone(), behavior.action))
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn idle_behavior_creates_or_returns() {
    let mut c = Config::default();
    c.idle.behavior.clear();
    idle_behavior(&mut c, "new").timeout = 42.;
    assert_eq!(c.idle.behavior["new"].timeout, 42.);
    assert_eq!(
      c.idle.behavior["new"].action,
      IdleBehavior::default().action
    );

    idle_behavior(&mut c, "new").command = "x".into();
    assert_eq!(c.idle.behavior.len(), 1);
    assert_eq!(c.idle.behavior["new"].timeout, 42.);
    assert_eq!(c.idle.behavior["new"].command, "x");
  }
}
