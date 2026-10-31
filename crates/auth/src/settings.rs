//! The lock screen page and the avatar on the system page

use std::iter;

use corona_components::settings_fields::{choice, non_empty, slider, switch, text};
use corona_compositor::{Compositor, types::Monitor};
use corona_config::ConfigProvider;
use corona_settings::{SettingsExt, SettingsPage, group, item, page};
use gpui_kit::{
  App, Axis,
  assets::IconName,
  component::setting::{SettingItem, SettingPage},
};
use rust_i18n::t;

pub struct LockscreenSettings;

impl SettingsPage for LockscreenSettings {
  const ID: &'static str = "lockscreen";
  const ORDER: i32 = 100;

  fn page(cx: &App) -> SettingPage {
    lockscreen(cx)
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<LockscreenSettings>()
    .group("system", "profile", 0, || {
      t!("app.settings.system.groups.profile").into()
    })
    .group_items("system", "profile", 0, avatar);
}

/// The picture on the lock screen and the dashboard
fn avatar(_: &App) -> Vec<SettingItem> {
  vec![
    item(
      t!("app.settings.system.avatar.title"),
      t!("app.settings.system.avatar.description"),
      text(
        "shell.avatar",
        |c| c.shell.avatar.clone().unwrap_or_default(),
        |c, v| c.shell.avatar = non_empty(v),
      ),
    )
    .layout(Axis::Vertical),
  ]
}

fn lockscreen(cx: &App) -> SettingPage {
  let connected = (cx.try_global::<Compositor>())
    .map(|c| c.list_monitors(cx))
    .unwrap_or_default();
  let monitors = monitor_choices(connected, cx.config().lockscreen.monitor.as_ref());
  page(
    t!("app.settings.lockscreen.title"),
    IconName::Lock,
    vec![group(
      t!("app.settings.lockscreen.groups.lock_screen"),
      vec![
        item(
          t!("app.settings.lockscreen.lock_before_suspend.title"),
          t!("app.settings.lockscreen.lock_before_suspend.description"),
          switch(
            |c| c.lockscreen.lock_before_suspend,
            |c, v| c.lockscreen.lock_before_suspend = v,
          ),
        ),
        item(
          t!("app.settings.lockscreen.blur.title"),
          t!("app.settings.lockscreen.blur.description"),
          slider(
            "lockscreen.blur",
            (0., 10., 0.5),
            |c| c.lockscreen.blur,
            |c, v| c.lockscreen.blur = v,
          ),
        ),
        item(
          t!("app.settings.lockscreen.monitor.title"),
          t!("app.settings.lockscreen.monitor.description"),
          choice(
            &iter::once((None, t!("app.settings.options.all_monitors")))
              .chain(monitors.into_iter().map(|m| (Some(m.clone()), m.into())))
              .collect::<Vec<_>>(),
            |c| c.lockscreen.monitor.clone(),
            |c, v| c.lockscreen.monitor = v,
          ),
        ),
      ],
    )],
  )
}

/// The enabled monitors' names, and a configured one that is not connected so
/// the dropdown still shows it
fn monitor_choices(connected: &[Monitor], configured: Option<&String>) -> Vec<String> {
  let mut names: Vec<String> = (connected.iter())
    .filter(|m| !m.disabled)
    .map(|m| m.name.clone())
    .collect();
  if let Some(configured) = configured
    && !names.contains(configured)
  {
    names.push(configured.clone());
  }
  names
}

#[cfg(test)]
mod tests {
  use corona_surface::testing::monitor;

  use super::*;

  #[test]
  fn monitor_choices_keep_the_configured() {
    let mut off = monitor("HDMI-A-1");
    off.disabled = true;
    let connected = [monitor("DP-1"), off];
    assert_eq!(monitor_choices(&connected, None), ["DP-1"]);
    assert_eq!(monitor_choices(&connected, Some(&"DP-1".into())), ["DP-1"]);
    // a disconnected or disabled choice stays selectable
    assert_eq!(
      monitor_choices(&connected, Some(&"DP-2".into())),
      ["DP-1", "DP-2"]
    );
    assert_eq!(
      monitor_choices(&connected, Some(&"HDMI-A-1".into())),
      ["DP-1", "HDMI-A-1"]
    );
    assert_eq!(monitor_choices(&[], Some(&"DP-2".into())), ["DP-2"]);
  }
}
