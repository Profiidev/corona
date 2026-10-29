//! The notifications page: the daemon, popups, battery warnings and filtered apps

use corona_components::settings_fields::{choice, number, slider, switch, text};
use corona_config::NotificationPosition;
use corona_settings::{SettingsExt, SettingsPage, group, item, page};
use gpui_kit::{App, Axis, assets::IconName, component::setting::SettingPage};
use rust_i18n::t;

const OPACITY: (f32, f32, f32) = (0., 1., 0.05);

pub struct NotificationSettings;

impl SettingsPage for NotificationSettings {
  const ID: &'static str = "notifications";
  const ORDER: i32 = 30;

  fn page(_: &App) -> SettingPage {
    notifications()
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut().register::<NotificationSettings>();
}

fn notifications() -> SettingPage {
  page(
    t!("app.settings.notifications.title"),
    IconName::Bell,
    vec![
      group(
        t!("app.settings.notifications.groups.daemon"),
        vec![item(
          t!("app.settings.notifications.notification_daemon.title"),
          t!("app.settings.notifications.notification_daemon.description"),
          switch(
            |c| c.notification.enabled,
            |c, v| c.notification.enabled = v,
          ),
        )],
      ),
      group(
        t!("app.settings.notifications.groups.popups"),
        vec![
          item(
            t!("app.settings.notifications.position.title"),
            t!("app.settings.notifications.position.description"),
            choice(
              &[
                (
                  NotificationPosition::TopLeft,
                  t!("app.settings.options.top_left"),
                ),
                (
                  NotificationPosition::TopRight,
                  t!("app.settings.options.top_right"),
                ),
              ],
              |c| c.notification.position,
              |c, v| c.notification.position = v,
            ),
          ),
          item(
            t!("app.settings.notifications.width.title"),
            t!("app.settings.notifications.width.description"),
            number(
              "notification.width",
              (200., 800., 10.),
              |c| c.notification.width.into(),
              |c, v| c.notification.width = v as f32,
            ),
          ),
          item(
            t!("app.settings.notifications.offset.title"),
            t!("app.settings.notifications.offset.description"),
            number(
              "notification.offset",
              (0., 200., 1.),
              |c| c.notification.offset.into(),
              |c, v| c.notification.offset = v as f32,
            ),
          ),
          item(
            t!("app.settings.notifications.timeout.title"),
            t!("app.settings.notifications.timeout.description"),
            number(
              "notification.timeout_ms",
              (500., 60000., 500.),
              |c| c.notification.timeout_ms as f64,
              |c, v| c.notification.timeout_ms = v as u64,
            ),
          ),
          item(
            t!("app.settings.notifications.critical_timeout.title"),
            t!("app.settings.notifications.critical_timeout.description"),
            number(
              "notification.critical_timeout_ms",
              (500., 120000., 500.),
              |c| c.notification.critical_timeout_ms as f64,
              |c, v| c.notification.critical_timeout_ms = v as u64,
            ),
          ),
          item(
            t!("app.settings.notifications.background_opacity.title"),
            t!("app.settings.notifications.background_opacity.description"),
            slider(
              "notification.background_opacity",
              OPACITY,
              |c| c.notification.background_opacity,
              |c, v| c.notification.background_opacity = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.notifications.groups.battery"),
        vec![
          item(
            t!("app.settings.notifications.battery_warnings.title"),
            t!("app.settings.notifications.battery_warnings.description"),
            switch(
              |c| c.notification.battery.enabled,
              |c, v| c.notification.battery.enabled = v,
            ),
          ),
          item(
            t!("app.settings.notifications.battery_low.title"),
            t!("app.settings.notifications.battery_low.description"),
            number(
              "notification.battery.low_percent",
              (0., 100., 1.),
              |c| c.notification.battery.low_percent.into(),
              |c, v| c.notification.battery.low_percent = v as u8,
            ),
          ),
          item(
            t!("app.settings.notifications.battery_critical.title"),
            t!("app.settings.notifications.battery_critical.description"),
            number(
              "notification.battery.critical_percent",
              (0., 100., 1.),
              |c| c.notification.battery.critical_percent.into(),
              |c, v| c.notification.battery.critical_percent = v as u8,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.notifications.groups.filter"),
        vec![
          item(
            t!("app.settings.notifications.filter.title"),
            t!("app.settings.notifications.filter.description"),
            text(
              "notification.filter_regex",
              |c| c.notification.filter_regex.clone(),
              |c, v| c.notification.filter_regex = v,
            ),
          )
          .layout(Axis::Vertical),
        ],
      ),
    ],
  )
}
