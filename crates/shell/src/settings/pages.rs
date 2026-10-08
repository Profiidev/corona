use std::{iter, path::PathBuf};

use corona_components::assets::{set_theme, theme_font, theme_names};
use corona_config::{
  Config, ConfigProvider, IdleAction, IdleBehavior, NotificationPosition, OsdPosition, ThemeMode,
  Units, Weekday,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Axis, SharedString, Styled, Window,
  assets::IconName,
  component::{
    Icon, Sizable,
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage},
  },
};

use crate::i18n;
use crate::settings::{
  bar,
  fields::{Choice, choice, number, optional_text, searchable, slider, switch, text},
};
use rust_i18n::t;

/// Page names, also what `corona ipc settings open <page>` takes, in sidebar order
pub const PAGES: [&str; 14] = [
  "appearance",
  "bar",
  "wallpaper",
  "notifications",
  "osd",
  "control_center",
  "taskbar",
  "window_switcher",
  "lockscreen",
  "idle",
  "screenshot",
  "location",
  "privacy",
  "system",
];

pub(super) fn all(cx: &App) -> Vec<SettingPage> {
  vec![
    appearance(cx),
    bar::page(cx),
    wallpaper(),
    notifications(),
    osd(),
    control_center(),
    taskbar(),
    window_switcher(),
    lockscreen(),
    idle(cx),
    screenshot(),
    location(),
    privacy(),
    system(),
  ]
}

fn page(title: impl Into<SharedString>, icon: IconName, groups: Vec<SettingGroup>) -> SettingPage {
  SettingPage::new(title)
    .icon(Icon::new(icon))
    .resettable(true)
    .groups(groups)
}

fn group(title: impl Into<SharedString>, items: Vec<SettingItem>) -> SettingGroup {
  SettingGroup::new().title(title).items(items)
}

fn item(
  title: impl Into<SharedString>,
  description: impl Into<String>,
  field: SettingField<impl Clone + PartialEq + Send + Sync + 'static>,
) -> SettingItem {
  SettingItem::new(title, field).description(description.into())
}

/// `Option<String>` settings as text, empty for unset
fn some(value: String) -> Option<String> {
  (!value.trim().is_empty()).then_some(value)
}

const OPACITY: (f32, f32, f32) = (0., 1., 0.05);

fn appearance(cx: &App) -> SettingPage {
  let themes: Vec<Choice> = theme_names(cx)
    .into_iter()
    .map(|name| Choice {
      value: name.to_string(),
      label: name.to_string(),
    })
    .collect();
  page(
    t!("app.settings.appearance.title"),
    IconName::Palette,
    vec![
      group(
        t!("app.settings.appearance.groups.theme"),
        vec![
          item(
            t!("app.settings.appearance.theme.title"),
            t!("app.settings.appearance.theme.description"),
            SettingField::render(
              move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
                let current = cx.config().theme.name.clone();
                searchable(
                  "theme.name",
                  themes.clone(),
                  Some(current),
                  t!("app.settings.appearance.theme.title"),
                  window,
                  cx,
                  |name, cx| {
                    if let Err(e) = set_theme(name.to_string(), cx) {
                      tracing::error!("Failed to set theme: {e:#}");
                    }
                  },
                )
                .with_size(options.size())
                .w_64()
              },
            )
            .on_reset(
              |cx: &App| cx.config().theme.name != Config::default().theme.name,
              |_, cx: &mut App| {
                let _ = set_theme(Config::default().theme.name, cx).log_err();
              },
            ),
          ),
          item(
            t!("app.settings.appearance.language.title"),
            t!("app.settings.appearance.language.description"),
            choice(
              // each language under its own name
              &iter::once((None, t!("app.settings.options.language_auto")))
                .chain(i18n::languages().into_iter().map(|language| {
                  let name = t!("app.language_name", locale = &language);
                  (Some(language), name)
                }))
                .collect::<Vec<_>>(),
              |c| c.shell.language.clone(),
              |c, v| c.shell.language = v,
            ),
          ),
          item(
            t!("app.settings.appearance.mode.title"),
            t!("app.settings.appearance.mode.description"),
            choice(
              &[
                (None, t!("app.settings.options.theme_default")),
                (Some(ThemeMode::Dark), t!("app.settings.options.dark")),
                (Some(ThemeMode::Light), t!("app.settings.options.light")),
              ],
              |c| c.theme.mode,
              |c, v| c.theme.mode = v,
            ),
          ),
          item(
            t!("app.settings.appearance.font_family.title"),
            t!("app.settings.appearance.font_family.description"),
            optional_text(
              "theme.font_family",
              |c| c.theme.font_family.clone(),
              |c, v| c.theme.font_family = v,
              |cx| theme_font(cx).to_string(),
            ),
          ),
          item(
            t!("app.settings.appearance.font_scale.title"),
            t!("app.settings.appearance.font_scale.description"),
            slider(
              "theme.font_scale",
              (0.5, 2., 0.05),
              |c| c.theme.font_scale,
              |c, v| c.theme.font_scale = v,
            ),
          ),
          item(
            t!("app.settings.appearance.corner_radius_scale.title"),
            t!("app.settings.appearance.corner_radius_scale.description"),
            slider(
              "theme.corner_radius_scale",
              (0., 3., 0.1),
              |c| c.theme.corner_radius_scale,
              |c, v| c.theme.corner_radius_scale = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.appearance.groups.surfaces"),
        vec![
          item(
            t!("app.settings.appearance.shadows.title"),
            t!("app.settings.appearance.shadows.description"),
            switch(|c| c.theme.shadow, |c, v| c.theme.shadow = v),
          ),
          item(
            t!("app.settings.appearance.popup_borders.title"),
            t!("app.settings.appearance.popup_borders.description"),
            switch(|c| c.theme.popup_borders, |c, v| c.theme.popup_borders = v),
          ),
          item(
            t!("app.settings.appearance.card_borders.title"),
            t!("app.settings.appearance.card_borders.description"),
            switch(|c| c.theme.card_borders, |c, v| c.theme.card_borders = v),
          ),
          item(
            t!("app.settings.appearance.screen_corners.title"),
            t!("app.settings.appearance.screen_corners.description"),
            switch(
              |c| c.theme.screen_corners,
              |c, v| c.theme.screen_corners = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.appearance.groups.animation"),
        vec![
          item(
            t!("app.settings.appearance.animations.title"),
            t!("app.settings.appearance.animations.description"),
            switch(
              |c| c.shell.animation.enabled,
              |c, v| c.shell.animation.enabled = v,
            ),
          ),
          item(
            t!("app.settings.appearance.speed.title"),
            t!("app.settings.appearance.speed.description"),
            slider(
              "shell.animation.speed",
              (0.25, 4., 0.25),
              |c| c.shell.animation.speed,
              |c, v| c.shell.animation.speed = v,
            ),
          ),
        ],
      ),
    ],
  )
}

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
    ],
  )
}

fn osd() -> SettingPage {
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
        vec![
          item(
            t!("app.settings.osd.volume.title"),
            t!("app.settings.osd.volume.description"),
            switch(|c| c.osd.kinds.volume, |c, v| c.osd.kinds.volume = v),
          ),
          item(
            t!("app.settings.osd.brightness.title"),
            t!("app.settings.osd.brightness.description"),
            switch(
              |c| c.osd.kinds.brightness,
              |c, v| c.osd.kinds.brightness = v,
            ),
          ),
          item(
            t!("app.settings.osd.wi_fi.title"),
            t!("app.settings.osd.wi_fi.description"),
            switch(|c| c.osd.kinds.wifi, |c, v| c.osd.kinds.wifi = v),
          ),
          item(
            t!("app.settings.osd.bluetooth.title"),
            t!("app.settings.osd.bluetooth.description"),
            switch(|c| c.osd.kinds.bluetooth, |c, v| c.osd.kinds.bluetooth = v),
          ),
          item(
            t!("app.settings.osd.do_not_disturb.title"),
            t!("app.settings.osd.do_not_disturb.description"),
            switch(|c| c.osd.kinds.dnd, |c, v| c.osd.kinds.dnd = v),
          ),
          item(
            t!("app.settings.osd.power_profile.title"),
            t!("app.settings.osd.power_profile.description"),
            switch(
              |c| c.osd.kinds.power_profile,
              |c, v| c.osd.kinds.power_profile = v,
            ),
          ),
          item(
            t!("app.settings.osd.privacy.title"),
            t!("app.settings.osd.privacy.description"),
            switch(|c| c.osd.kinds.privacy, |c, v| c.osd.kinds.privacy = v),
          ),
          item(
            t!("app.settings.osd.keyboard_layout.title"),
            t!("app.settings.osd.keyboard_layout.description"),
            switch(
              |c| c.osd.kinds.keyboard_layout,
              |c, v| c.osd.kinds.keyboard_layout = v,
            ),
          ),
          item(
            t!("app.settings.osd.lock_keys.title"),
            t!("app.settings.osd.lock_keys.description"),
            switch(|c| c.osd.kinds.lock_keys, |c, v| c.osd.kinds.lock_keys = v),
          ),
        ],
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
      ],
    )],
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

fn lockscreen() -> SettingPage {
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
      ],
    )],
  )
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
              crate::overlays::screenshot::save::default_directory()
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
        vec![
          item(
            t!("app.settings.system.ddcutil.title"),
            t!("app.settings.system.ddcutil.description"),
            switch(
              |c| c.brightness.enable_ddcutil,
              |c, v| c.brightness.enable_ddcutil = v,
            ),
          ),
          item(
            t!("app.settings.system.plugin_directory.title"),
            t!("app.settings.system.plugin_directory.description"),
            optional_text(
              "shell.plugin_dir",
              |c| {
                c.shell
                  .plugin_dir
                  .as_ref()
                  .map(|p| p.to_string_lossy().into_owned())
              },
              |c, v| c.shell.plugin_dir = v.map(PathBuf::from),
              |_| {
                corona_config::ShellConfig::default()
                  .plugin_dir()
                  .to_string_lossy()
                  .into_owned()
              },
            ),
          )
          .layout(Axis::Vertical),
        ],
      ),
    ],
  )
}
