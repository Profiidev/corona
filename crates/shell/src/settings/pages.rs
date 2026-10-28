use std::{iter, path::PathBuf};

use corona_compositor::{Compositor, types::Monitor};
use corona_config::{
  Config, ConfigProvider, IdleAction, IdleBehavior, NightlightMode, NotificationPosition,
  OsdPosition, Units, Weekday,
  osk::{AutoShow, OskLanguage, OskLayout, TabletSource},
};
use corona_osk::Osk;
use corona_surface::{
  control_center::{ControlCenterExt, DASHBOARD},
  dashboard::DashboardExt,
  osd::OsdExt,
};
use gpui_kit::{
  App, Axis, BorrowAppContext,
  assets::IconName,
  component::{
    Sizable,
    button::Button,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
  },
};

use corona_components::settings_fields::{
  Choice, choice, number, optional_text,
  order::{OrderList, order_item},
  slider, switch, text,
};

use corona_settings::{SettingsExt, SettingsPage, group, item, page};

use crate::settings::plugins;
use rust_i18n::t;

/// A settings page `$page` builds
macro_rules! settings_page {
  ($name:ident, $id:literal, $order:literal, |$cx:ident| $page:expr) => {
    pub struct $name;

    impl SettingsPage for $name {
      const ID: &'static str = $id;
      const ORDER: i32 = $order;

      #[allow(unused_variables)]
      fn page($cx: &App) -> SettingPage {
        $page
      }
    }
  };
}

// the order is the sidebar's, between the bar (10) and launcher (80) pages of
// their own crates
settings_page!(WallpaperSettings, "wallpaper", 20, |cx| wallpaper());
settings_page!(NotificationSettings, "notifications", 30, |cx| {
  notifications()
});
settings_page!(OsdSettings, "osd", 40, |cx| osd(cx));
settings_page!(ControlCenterSettings, "control_center", 50, |cx| {
  control_center()
});
settings_page!(TaskbarSettings, "taskbar", 60, |cx| taskbar());
settings_page!(WindowSwitcherSettings, "window_switcher", 70, |cx| {
  window_switcher()
});
settings_page!(OskSettings, "osk", 90, |cx| osk());
settings_page!(LockscreenSettings, "lockscreen", 100, |cx| lockscreen(cx));
settings_page!(IdleSettings, "idle", 110, |cx| idle(cx));
settings_page!(ScreenshotSettings, "screenshot", 120, |cx| screenshot());
settings_page!(LocationSettings, "location", 130, |cx| location());
settings_page!(PrivacySettings, "privacy", 140, |cx| privacy());
settings_page!(PluginSettings, "plugins", 150, |cx| plugins::page(cx));
settings_page!(SystemSettings, "system", 160, |cx| system());

pub(super) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<WallpaperSettings>()
    .register::<NotificationSettings>()
    .register::<OsdSettings>()
    .register::<ControlCenterSettings>()
    .register::<TaskbarSettings>()
    .register::<WindowSwitcherSettings>()
    .register::<OskSettings>()
    .register::<LockscreenSettings>()
    .register::<IdleSettings>()
    .register::<ScreenshotSettings>()
    .register::<LocationSettings>()
    .register::<PrivacySettings>()
    .register::<PluginSettings>()
    .register::<SystemSettings>();
}

/// `Option<String>` settings as text, empty for unset
fn some(value: String) -> Option<String> {
  (!value.trim().is_empty()).then_some(value)
}

const OPACITY: (f32, f32, f32) = (0., 1., 0.05);

fn wallpaper() -> SettingPage {
  page(
    t!("app.settings.wallpaper.title"),
    IconName::Image,
    vec![group(
      t!("app.settings.wallpaper.groups.wallpaper"),
      vec![
        item(
          t!("app.settings.wallpaper.picture.title"),
          t!("app.settings.wallpaper.picture.description"),
          text(
            "wallpaper.path",
            |c| c.wallpaper.path.clone().unwrap_or_default(),
            |c, v| c.wallpaper.path = some(v),
          ),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
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

fn osd(cx: &App) -> SettingPage {
  page(
    t!("app.settings.osd.title"),
    IconName::Gauge,
    vec![
      group(
        t!("app.settings.osd.groups.on_screen_display"),
        vec![
          item(
            t!("app.settings.osd.osd.title"),
            t!("app.settings.osd.osd.description"),
            switch(|c| c.osd.enabled, |c, v| c.osd.enabled = v),
          ),
          item(
            t!("app.settings.osd.position.title"),
            t!("app.settings.osd.position.description"),
            choice(
              &[
                (OsdPosition::TopCenter, t!("app.settings.options.top")),
                (OsdPosition::BottomCenter, t!("app.settings.options.bottom")),
                (OsdPosition::CenterLeft, t!("app.settings.options.left")),
                (OsdPosition::CenterRight, t!("app.settings.options.right")),
              ],
              |c| c.osd.position,
              |c, v| c.osd.position = v,
            ),
          ),
          item(
            t!("app.settings.osd.offset.title"),
            t!("app.settings.osd.offset.description"),
            number(
              "osd.offset",
              (0., 400., 1.),
              |c| c.osd.offset.into(),
              |c, v| c.osd.offset = v as f32,
            ),
          ),
          item(
            t!("app.settings.osd.hide_after.title"),
            t!("app.settings.osd.hide_after.description"),
            number(
              "osd.hide_delay_ms",
              (250., 10000., 250.),
              |c| c.osd.hide_delay_ms as f64,
              |c, v| c.osd.hide_delay_ms = v as u64,
            ),
          ),
          item(
            t!("app.settings.osd.background_opacity.title"),
            t!("app.settings.osd.background_opacity.description"),
            slider(
              "osd.background_opacity",
              OPACITY,
              |c| c.osd.background_opacity,
              |c, v| c.osd.background_opacity = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.osd.groups.shown_for"),
        cx.osds()
          .iter()
          .map(|kind| {
            let id = kind.id;
            item(
              (kind.title)(),
              (kind.description)(),
              switch(
                move |c| c.osd.kinds.get(id).copied().unwrap_or(true),
                move |c, v| {
                  c.osd.kinds.insert(id.to_string(), v);
                },
              ),
            )
          })
          .collect(),
      ),
    ],
  )
}

fn control_center() -> SettingPage {
  page(
    t!("app.settings.control_center.title"),
    IconName::LayoutDashboard,
    vec![group(
      t!("app.settings.control_center.groups.control_center"),
      vec![
        item(
          t!("app.settings.control_center.time_format.title"),
          t!("app.settings.control_center.time_format.description"),
          text(
            "control_center.time_format",
            |c| c.control_center.time_format.clone(),
            |c, v| c.control_center.time_format = v,
          ),
        ),
        item(
          t!("app.settings.control_center.date_format.title"),
          t!("app.settings.control_center.date_format.description"),
          text(
            "control_center.date_format",
            |c| c.control_center.date_format.clone(),
            |c, v| c.control_center.date_format = v,
          ),
        ),
        item(
          t!("app.settings.control_center.week_starts_on.title"),
          t!("app.settings.control_center.week_starts_on.description"),
          choice(
            &[
              (Weekday::Monday, t!("app.settings.options.monday")),
              (Weekday::Sunday, t!("app.settings.options.sunday")),
            ],
            |c| c.control_center.week_start,
            |c, v| c.control_center.week_start = v,
          ),
        ),
        item(
          t!("app.settings.control_center.background_opacity.title"),
          t!("app.settings.control_center.background_opacity.description"),
          slider(
            "control_center.background_opacity",
            OPACITY,
            |c| c.control_center.background_opacity,
            |c, v| c.control_center.background_opacity = v,
          ),
        ),
        order_item([
          OrderList {
            key: "control_center.sidebar",
            title: || t!("app.settings.control_center.sidebar.title"),
            description: || t!("app.settings.control_center.sidebar.description"),
            options: |cx| {
              (cx.control_center().pages().iter())
                .filter(|p| p.id != DASHBOARD)
                .map(|p| Choice {
                  value: p.id.into(),
                  label: (p.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.sidebar.clone(),
            set: |c, v| c.control_center.sidebar = v,
          },
          OrderList {
            key: "control_center.quick_actions",
            title: || t!("app.settings.control_center.quick_actions.title"),
            description: || t!("app.settings.control_center.quick_actions.description"),
            options: |cx| {
              (cx.dashboard().quick_actions().iter())
                .map(|a| Choice {
                  value: a.id.into(),
                  label: (a.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.quick_actions.clone(),
            set: |c, v| c.control_center.quick_actions = v,
          },
        ]),
        order_item([
          OrderList {
            key: "control_center.sliders",
            title: || t!("app.settings.control_center.sliders.title"),
            description: || t!("app.settings.control_center.sliders.description"),
            options: |cx| {
              (cx.dashboard().sliders().iter())
                .map(|s| Choice {
                  value: s.id.into(),
                  label: (s.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.sliders.clone(),
            set: |c, v| c.control_center.sliders = v,
          },
          OrderList {
            key: "control_center.cards",
            title: || t!("app.settings.control_center.cards.title"),
            description: || t!("app.settings.control_center.cards.description"),
            options: |cx| {
              (cx.dashboard().cards().iter())
                .map(|c| Choice {
                  value: c.id.into(),
                  label: (c.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.cards.clone(),
            set: |c, v| c.control_center.cards = v,
          },
        ]),
      ],
    )],
  )
}

fn osk() -> SettingPage {
  page(
    t!("app.settings.osk.title"),
    IconName::Keyboard,
    vec![
      group(
        t!("app.settings.osk.groups.keyboard"),
        vec![
          item(
            t!("app.settings.osk.enabled.title"),
            t!("app.settings.osk.enabled.description"),
            switch(|c| c.osk.enabled, |c, v| c.osk.enabled = v),
          ),
          item(
            t!("app.settings.osk.auto_show.title"),
            t!("app.settings.osk.auto_show.description"),
            choice(
              &[
                (AutoShow::Never, t!("app.settings.options.never")),
                (AutoShow::Always, t!("app.settings.options.always")),
                (AutoShow::TabletMode, t!("app.settings.options.tablet_mode")),
              ],
              |c| c.osk.auto_show,
              |c, v| c.osk.auto_show = v,
            ),
          ),
          item(
            t!("app.settings.osk.tablet_source.title"),
            t!("app.settings.osk.tablet_source.description"),
            choice(
              &[
                (TabletSource::Ipc, t!("app.settings.options.ipc")),
                (TabletSource::Hinge, t!("app.settings.options.hinge")),
              ],
              |c| c.osk.tablet_source,
              |c, v| c.osk.tablet_source = v,
            ),
          ),
          item(
            t!("app.settings.osk.layout.title"),
            t!("app.settings.osk.layout.description"),
            choice(
              &[
                (OskLayout::Auto, t!("app.settings.options.language_auto")),
                (OskLayout::Qwerty, "QWERTY".into()),
                (OskLayout::Qwertz, "QWERTZ".into()),
              ],
              |c| c.osk.layout,
              |c, v| c.osk.layout = v,
            ),
          ),
          item(
            t!("app.settings.osk.language.title"),
            t!("app.settings.osk.language.description"),
            // each language under its own name
            choice(
              &[
                (OskLanguage::Auto, t!("app.settings.options.language_auto")),
                (OskLanguage::En, "English".into()),
                (OskLanguage::De, "Deutsch".into()),
              ],
              |c| c.osk.language,
              |c, v| c.osk.language = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.osk.groups.suggestions"),
        vec![
          item(
            t!("app.settings.osk.suggestions.title"),
            t!("app.settings.osk.suggestions.description"),
            switch(|c| c.osk.suggestions, |c, v| c.osk.suggestions = v),
          ),
          item(
            t!("app.settings.osk.spell_check.title"),
            t!("app.settings.osk.spell_check.description"),
            switch(|c| c.osk.spell_check, |c, v| c.osk.spell_check = v),
          ),
          item(
            t!("app.settings.osk.learn_words.title"),
            t!("app.settings.osk.learn_words.description"),
            switch(|c| c.osk.learn_words, |c, v| c.osk.learn_words = v),
          ),
          SettingItem::new(
            t!("app.settings.osk.clear_learned.title"),
            SettingField::render(|_, _, _| {
              Button::new("osk-clear-learned")
                .label(t!("app.settings.osk.clear_learned.button"))
                .outline()
                .small()
                .on_click(|_, _, cx| {
                  if cx.has_global::<Osk>() {
                    cx.update_global::<Osk, _>(|osk, cx| osk.clear_learned(cx));
                  }
                })
            }),
          )
          .description(t!("app.settings.osk.clear_learned.description").to_string()),
        ],
      ),
    ],
  )
}

fn taskbar() -> SettingPage {
  page(
    t!("app.settings.taskbar.title"),
    IconName::PanelBottom,
    vec![group(
      t!("app.settings.taskbar.groups.taskbar"),
      vec![
        item(
          t!("app.settings.taskbar.taskbar.title"),
          t!("app.settings.taskbar.taskbar.description"),
          switch(|c| c.taskbar.enabled, |c, v| c.taskbar.enabled = v),
        ),
        item(
          t!("app.settings.taskbar.icon_size.title"),
          t!("app.settings.taskbar.icon_size.description"),
          number(
            "taskbar.icon_size",
            (16., 96., 2.),
            |c| c.taskbar.icon_size.into(),
            |c, v| c.taskbar.icon_size = v as f32,
          ),
        ),
        item(
          t!("app.settings.taskbar.window_previews.title"),
          t!("app.settings.taskbar.window_previews.description"),
          switch(|c| c.taskbar.previews, |c, v| c.taskbar.previews = v),
        ),
        item(
          t!("app.settings.taskbar.previews_at_most.title"),
          t!("app.settings.taskbar.previews_at_most.description"),
          number(
            "taskbar.preview_max_windows",
            (1., 20., 1.),
            |c| c.taskbar.preview_max_windows as f64,
            |c, v| c.taskbar.preview_max_windows = v as usize,
          ),
        ),
        item(
          t!("app.settings.taskbar.background_opacity.title"),
          t!("app.settings.taskbar.background_opacity.description"),
          slider(
            "taskbar.background_opacity",
            OPACITY,
            |c| c.taskbar.background_opacity,
            |c, v| c.taskbar.background_opacity = v,
          ),
        ),
      ],
    )],
  )
}

fn window_switcher() -> SettingPage {
  page(
    t!("app.settings.window_switcher.title"),
    IconName::AppWindow,
    vec![group(
      t!("app.settings.window_switcher.groups.window_switcher"),
      vec![
        item(
          t!("app.settings.window_switcher.focused_monitor_only.title"),
          t!("app.settings.window_switcher.focused_monitor_only.description"),
          switch(
            |c| c.window_switcher.current_monitor_only,
            |c, v| c.window_switcher.current_monitor_only = v,
          ),
        ),
        item(
          t!("app.settings.window_switcher.card_height.title"),
          t!("app.settings.window_switcher.card_height.description"),
          number(
            "window_switcher.card_height",
            (80., 480., 10.),
            |c| c.window_switcher.card_height.into(),
            |c, v| c.window_switcher.card_height = v as f32,
          ),
        ),
        item(
          t!("app.settings.window_switcher.backdrop.title"),
          t!("app.settings.window_switcher.backdrop.description"),
          slider(
            "window_switcher.backdrop_opacity",
            OPACITY,
            |c| c.window_switcher.backdrop_opacity,
            |c, v| c.window_switcher.backdrop_opacity = v,
          ),
        ),
      ],
    )],
  )
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

fn screenshot() -> SettingPage {
  page(
    t!("app.settings.screenshot.title"),
    IconName::Camera,
    vec![group(
      t!("app.settings.screenshot.groups.saving"),
      vec![
        item(
          t!("app.settings.screenshot.directory.title"),
          t!("app.settings.screenshot.directory.description"),
          optional_text(
            "screenshot.directory",
            |c| {
              c.screenshot
                .directory
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
            },
            |c, v| c.screenshot.directory = v.map(PathBuf::from),
            |_| {
              corona_config::ScreenshotConfig::default()
                .resolved_directory()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
            },
          ),
        )
        .layout(Axis::Vertical),
        item(
          t!("app.settings.screenshot.file_name.title"),
          t!("app.settings.screenshot.file_name.description"),
          text(
            "screenshot.filename_pattern",
            |c| c.screenshot.filename_pattern.clone(),
            |c, v| c.screenshot.filename_pattern = v,
          ),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
}

fn location() -> SettingPage {
  page(
    t!("app.settings.location.title"),
    IconName::CloudSun,
    vec![
      group(
        t!("app.settings.location.groups.location"),
        vec![
          item(
            t!("app.settings.location.city.title"),
            t!("app.settings.location.city.description"),
            text(
              "location.city",
              |c| c.location.city.clone().unwrap_or_default(),
              |c, v| c.location.city = some(v),
            ),
          ),
          item(
            t!("app.settings.location.coordinates.title"),
            t!("app.settings.location.coordinates.description"),
            text(
              "location.coordinates",
              |c| match (c.location.latitude, c.location.longitude) {
                (Some(lat), Some(lon)) => format!("{lat}, {lon}"),
                _ => String::new(),
              },
              |c, v| {
                let parsed = v.split_once(',').and_then(|(lat, lon)| {
                  Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
                });
                (c.location.latitude, c.location.longitude) = match parsed {
                  Some((lat, lon)) => (Some(lat), Some(lon)),
                  None => (None, None),
                };
              },
            ),
          ),
          item(
            t!("app.settings.location.locate_automatically.title"),
            t!("app.settings.location.locate_automatically.description"),
            switch(
              |c| c.location.auto_locate,
              |c, v| c.location.auto_locate = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.location.groups.weather"),
        vec![
          item(
            t!("app.settings.location.units.title"),
            t!("app.settings.location.units.description"),
            choice(
              &[
                (Units::Metric, t!("app.settings.options.metric")),
                (Units::Imperial, t!("app.settings.options.imperial")),
              ],
              |c| c.weather.units,
              |c, v| c.weather.units = v,
            ),
          ),
          item(
            t!("app.settings.location.refresh_every.title"),
            t!("app.settings.location.refresh_every.description"),
            number(
              "weather.refresh_minutes",
              (5., 240., 5.),
              |c| c.weather.refresh_minutes as f64,
              |c, v| c.weather.refresh_minutes = v as u64,
            ),
          ),
        ],
      ),
    ],
  )
}

fn privacy() -> SettingPage {
  page(
    t!("app.settings.privacy.title"),
    IconName::Shield,
    vec![group(
      t!("app.settings.privacy.groups.hidden_apps"),
      vec![
        item(
          t!("app.settings.privacy.microphone.title"),
          t!("app.settings.privacy.microphone.description"),
          text(
            "shell.privacy.mic",
            |c| c.shell.privacy.mic_filter_regex.clone(),
            |c, v| c.shell.privacy.mic_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
        item(
          t!("app.settings.privacy.camera.title"),
          t!("app.settings.privacy.camera.description"),
          text(
            "shell.privacy.cam",
            |c| c.shell.privacy.cam_filter_regex.clone(),
            |c, v| c.shell.privacy.cam_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
        item(
          t!("app.settings.privacy.screen.title"),
          t!("app.settings.privacy.screen.description"),
          text(
            "shell.privacy.screen",
            |c| c.shell.privacy.screen_filter_regex.clone(),
            |c, v| c.shell.privacy.screen_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
}

fn system() -> SettingPage {
  page(
    t!("app.settings.system.title"),
    IconName::Cpu,
    vec![
      group(
        t!("app.settings.system.groups.profile"),
        vec![
          item(
            t!("app.settings.system.avatar.title"),
            t!("app.settings.system.avatar.description"),
            text(
              "shell.avatar",
              |c| c.shell.avatar.clone().unwrap_or_default(),
              |c, v| c.shell.avatar = some(v),
            ),
          )
          .layout(Axis::Vertical),
        ],
      ),
      group(
        t!("app.settings.system.groups.nightlight"),
        vec![
          item(
            t!("app.settings.system.nightlight.mode.title"),
            t!("app.settings.system.nightlight.mode.description"),
            choice(
              &[
                (
                  NightlightMode::Manual,
                  t!("app.settings.options.nightlight_manual"),
                ),
                (
                  NightlightMode::Solar,
                  t!("app.settings.options.nightlight_solar"),
                ),
                (
                  NightlightMode::Schedule,
                  t!("app.settings.options.nightlight_schedule"),
                ),
              ],
              |c| c.nightlight.mode,
              |c, v| c.nightlight.mode = v,
            ),
          ),
          item(
            t!("app.settings.system.nightlight.temperature.title"),
            t!("app.settings.system.nightlight.temperature.description"),
            number(
              "nightlight.temperature",
              (1000., 6500., 100.),
              |c| c.nightlight.temperature as f64,
              |c, v| c.nightlight.temperature = v as u32,
            ),
          ),
          item(
            t!("app.settings.system.nightlight.start.title"),
            t!("app.settings.system.nightlight.start.description"),
            text(
              "nightlight.start",
              |c| c.nightlight.start.clone(),
              |c, v| c.nightlight.start = v,
            ),
          ),
          item(
            t!("app.settings.system.nightlight.end.title"),
            t!("app.settings.system.nightlight.end.description"),
            text(
              "nightlight.end",
              |c| c.nightlight.end.clone(),
              |c, v| c.nightlight.end = v,
            ),
          ),
          item(
            t!("app.settings.system.nightlight.transition.title"),
            t!("app.settings.system.nightlight.transition.description"),
            number(
              "nightlight.transition_minutes",
              (0., 180., 1.),
              |c| c.nightlight.transition_minutes as f64,
              |c, v| c.nightlight.transition_minutes = v as u32,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.system.groups.polling"),
        vec![
          item(
            t!("app.settings.system.system_monitor.title"),
            t!("app.settings.system.system_monitor.description"),
            number(
              "system.monitor.poll_seconds",
              (1., 60., 1.),
              |c| c.system.monitor.poll_seconds as f64,
              |c, v| c.system.monitor.poll_seconds = v as u64,
            ),
          ),
          item(
            t!("app.settings.system.brightness.title"),
            t!("app.settings.system.brightness.description"),
            number(
              "brightness.poll_seconds",
              (1., 60., 1.),
              |c| c.brightness.poll_seconds as f64,
              |c, v| c.brightness.poll_seconds = v as u64,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.system.groups.applies_after_a_restart"),
        vec![item(
          t!("app.settings.system.ddcutil.title"),
          t!("app.settings.system.ddcutil.description"),
          switch(
            |c| c.brightness.enable_ddcutil,
            |c, v| c.brightness.enable_ddcutil = v,
          ),
        )],
      ),
    ],
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn monitor_choices_keep_the_configured() {
    let mut off = crate::test_support::monitor("HDMI-A-1");
    off.disabled = true;
    let connected = [crate::test_support::monitor("DP-1"), off];
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

  #[test]
  fn some_is_none_for_blank() {
    assert_eq!(some(String::new()), None);
    assert_eq!(some("  ".into()), None);
    assert_eq!(some("x".into()), Some("x".into()));
    // kept as typed
    assert_eq!(some(" x ".into()), Some(" x ".into()));
  }

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
