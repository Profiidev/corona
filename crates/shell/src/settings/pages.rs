use std::path::PathBuf;

use corona_components::assets::{set_theme, theme_font, theme_names};
use corona_config::{
  Config, ConfigProvider, NotificationPosition, OsdPosition, ThemeMode, Units, Weekday,
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

use crate::settings::{
  bar,
  fields::{Choice, choice, number, optional_text, searchable, slider, switch, text},
};

/// Page names, also what `corona ipc settings open <page>` takes, in sidebar order
pub const PAGES: [&str; 13] = [
  "appearance",
  "bar",
  "wallpaper",
  "notifications",
  "osd",
  "control_center",
  "taskbar",
  "window_switcher",
  "lockscreen",
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
    screenshot(),
    location(),
    privacy(),
    system(),
  ]
}

fn page(title: &'static str, icon: IconName, groups: Vec<SettingGroup>) -> SettingPage {
  SettingPage::new(title)
    .icon(Icon::new(icon))
    .resettable(true)
    .groups(groups)
}

fn group(title: &'static str, items: Vec<SettingItem>) -> SettingGroup {
  SettingGroup::new().title(title).items(items)
}

fn item(
  title: &'static str,
  description: &'static str,
  field: SettingField<impl Clone + PartialEq + Send + Sync + 'static>,
) -> SettingItem {
  let item = SettingItem::new(title, field);
  match description.is_empty() {
    true => item,
    false => item.description(description),
  }
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
      value: name.clone().into(),
      label: name.into(),
    })
    .collect();
  page(
    "Appearance",
    IconName::Palette,
    vec![
      group(
        "Theme",
        vec![
          item(
            "Theme",
            "Its Dark/Light counterpart serves the other mode.",
            SettingField::render(
              move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
                let current: SharedString = cx.config().theme.name.clone().into();
                searchable(
                  "theme.name",
                  themes.clone(),
                  Some(current),
                  "Theme",
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
            "Mode",
            "Follow the theme, or force dark or light.",
            choice(
              &[
                (None, "Theme's own"),
                (Some(ThemeMode::Dark), "Dark"),
                (Some(ThemeMode::Light), "Light"),
              ],
              |c| c.theme.mode,
              |c, v| c.theme.mode = v,
            ),
          ),
          item(
            "Font family",
            "Used for all text.",
            optional_text(
              "theme.font_family",
              |c| c.theme.font_family.clone(),
              |c, v| c.theme.font_family = v,
              |cx| theme_font(cx).to_string(),
            ),
          ),
          item(
            "Font scale",
            "Text, and the spacing measured in it.",
            slider(
              "theme.font_scale",
              (0.5, 2., 0.05),
              |c| c.theme.font_scale,
              |c, v| c.theme.font_scale = v,
            ),
          ),
          item(
            "Corner radius scale",
            "0 for square corners.",
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
        "Surfaces",
        vec![
          item(
            "Shadows",
            "Under popovers.",
            switch(|c| c.theme.shadow, |c, v| c.theme.shadow = v),
          ),
          item(
            "Popup borders",
            "Around OSDs, tooltips, menus and the window switcher.",
            switch(|c| c.theme.popup_borders, |c, v| c.theme.popup_borders = v),
          ),
          item(
            "Card borders",
            "Around the cards in panels.",
            switch(|c| c.theme.card_borders, |c, v| c.theme.card_borders = v),
          ),
        ],
      ),
      group(
        "Animation",
        vec![
          item(
            "Animations",
            "Turn every animation off, or on.",
            switch(
              |c| c.shell.animation.enabled,
              |c, v| c.shell.animation.enabled = v,
            ),
          ),
          item(
            "Speed",
            "2 is twice as fast.",
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
    "Wallpaper",
    IconName::Image,
    vec![group(
      "Wallpaper",
      vec![
        item(
          "Picture",
          "Path, ~/ path or http(s) URL.",
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
    "Notifications",
    IconName::Bell,
    vec![
      group(
        "Daemon",
        vec![item(
          "Notification daemon",
          "Applies after a restart.",
          switch(
            |c| c.notification.enabled,
            |c, v| c.notification.enabled = v,
          ),
        )],
      ),
      group(
        "Popups",
        vec![
          item(
            "Position",
            "Which top corner notifications appear in.",
            choice(
              &[
                (NotificationPosition::TopLeft, "Top left"),
                (NotificationPosition::TopRight, "Top right"),
              ],
              |c| c.notification.position,
              |c, v| c.notification.position = v,
            ),
          ),
          item(
            "Width",
            "Of a notification popup, in pixels.",
            number(
              "notification.width",
              (200., 800., 10.),
              |c| c.notification.width.into(),
              |c, v| c.notification.width = v as f32,
            ),
          ),
          item(
            "Offset",
            "Distance from the screen edges.",
            number(
              "notification.offset",
              (0., 200., 1.),
              |c| c.notification.offset.into(),
              |c, v| c.notification.offset = v as f32,
            ),
          ),
          item(
            "Timeout",
            "Milliseconds.",
            number(
              "notification.timeout_ms",
              (500., 60000., 500.),
              |c| c.notification.timeout_ms as f64,
              |c, v| c.notification.timeout_ms = v as u64,
            ),
          ),
          item(
            "Critical timeout",
            "Milliseconds.",
            number(
              "notification.critical_timeout_ms",
              (500., 120000., 500.),
              |c| c.notification.critical_timeout_ms as f64,
              |c, v| c.notification.critical_timeout_ms = v as u64,
            ),
          ),
          item(
            "Background opacity",
            "Of notification popups; under 1 lets the desktop show through.",
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
    "OSD",
    IconName::Gauge,
    vec![
      group(
        "On-screen display",
        vec![
          item(
            "OSD",
            "Show a popup when volume, brightness and the like change.",
            switch(|c| c.osd.enabled, |c, v| c.osd.enabled = v),
          ),
          item(
            "Position",
            "Which screen edge the OSD sits at.",
            choice(
              &[
                (OsdPosition::TopCenter, "Top"),
                (OsdPosition::BottomCenter, "Bottom"),
                (OsdPosition::CenterLeft, "Left"),
                (OsdPosition::CenterRight, "Right"),
              ],
              |c| c.osd.position,
              |c, v| c.osd.position = v,
            ),
          ),
          item(
            "Offset",
            "Distance from the screen edge.",
            number(
              "osd.offset",
              (0., 400., 1.),
              |c| c.osd.offset.into(),
              |c, v| c.osd.offset = v as f32,
            ),
          ),
          item(
            "Hide after",
            "Milliseconds.",
            number(
              "osd.hide_delay_ms",
              (250., 10000., 250.),
              |c| c.osd.hide_delay_ms as f64,
              |c, v| c.osd.hide_delay_ms = v as u64,
            ),
          ),
          item(
            "Background opacity",
            "Of the OSD; under 1 lets the desktop show through.",
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
        "Shown for",
        vec![
          item(
            "Volume",
            "When output or input volume changes.",
            switch(|c| c.osd.kinds.volume, |c, v| c.osd.kinds.volume = v),
          ),
          item(
            "Brightness",
            "When screen brightness changes.",
            switch(
              |c| c.osd.kinds.brightness,
              |c, v| c.osd.kinds.brightness = v,
            ),
          ),
          item(
            "Wi-Fi",
            "When Wi-Fi is turned on or off.",
            switch(|c| c.osd.kinds.wifi, |c, v| c.osd.kinds.wifi = v),
          ),
          item(
            "Bluetooth",
            "When Bluetooth is turned on or off.",
            switch(|c| c.osd.kinds.bluetooth, |c, v| c.osd.kinds.bluetooth = v),
          ),
          item(
            "Do not disturb",
            "When do not disturb is turned on or off.",
            switch(|c| c.osd.kinds.dnd, |c, v| c.osd.kinds.dnd = v),
          ),
          item(
            "Power profile",
            "When the power profile changes.",
            switch(
              |c| c.osd.kinds.power_profile,
              |c, v| c.osd.kinds.power_profile = v,
            ),
          ),
          item(
            "Privacy",
            "Microphone, camera and screen access.",
            switch(|c| c.osd.kinds.privacy, |c, v| c.osd.kinds.privacy = v),
          ),
        ],
      ),
    ],
  )
}

fn control_center() -> SettingPage {
  page(
    "Control Center",
    IconName::LayoutDashboard,
    vec![group(
      "Control center",
      vec![
        item(
          "Time format",
          "strftime pattern of the dashboard clock.",
          text(
            "control_center.time_format",
            |c| c.control_center.time_format.clone(),
            |c, v| c.control_center.time_format = v,
          ),
        ),
        item(
          "Date format",
          "strftime pattern.",
          text(
            "control_center.date_format",
            |c| c.control_center.date_format.clone(),
            |c, v| c.control_center.date_format = v,
          ),
        ),
        item(
          "Week starts on",
          "The first column of the calendar.",
          choice(
            &[(Weekday::Monday, "Monday"), (Weekday::Sunday, "Sunday")],
            |c| c.control_center.week_start,
            |c, v| c.control_center.week_start = v,
          ),
        ),
        item(
          "Background opacity",
          "Of the panels.",
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
    "Taskbar",
    IconName::PanelBottom,
    vec![group(
      "Taskbar",
      vec![
        item(
          "Taskbar",
          "The dock that rises from the bottom edge.",
          switch(|c| c.taskbar.enabled, |c, v| c.taskbar.enabled = v),
        ),
        item(
          "Icon size",
          "Of the app icons, in pixels.",
          number(
            "taskbar.icon_size",
            (16., 96., 2.),
            |c| c.taskbar.icon_size.into(),
            |c, v| c.taskbar.icon_size = v as f32,
          ),
        ),
        item(
          "Window previews",
          "Show live previews of an app's windows on hover.",
          switch(|c| c.taskbar.previews, |c, v| c.taskbar.previews = v),
        ),
        item(
          "Previews at most",
          "Windows shown in one preview.",
          number(
            "taskbar.preview_max_windows",
            (1., 20., 1.),
            |c| c.taskbar.preview_max_windows as f64,
            |c, v| c.taskbar.preview_max_windows = v as usize,
          ),
        ),
        item(
          "Background opacity",
          "Of the taskbar; under 1 lets the desktop show through.",
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
    "Window Switcher",
    IconName::AppWindow,
    vec![group(
      "Window switcher",
      vec![
        item(
          "Focused monitor only",
          "Even without --current-monitor.",
          switch(
            |c| c.window_switcher.current_monitor_only,
            |c, v| c.window_switcher.current_monitor_only = v,
          ),
        ),
        item(
          "Card height",
          "Of each workspace card, in pixels.",
          number(
            "window_switcher.card_height",
            (80., 480., 10.),
            |c| c.window_switcher.card_height.into(),
            |c, v| c.window_switcher.card_height = v as f32,
          ),
        ),
        item(
          "Backdrop",
          "How much the screen behind darkens.",
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

fn lockscreen() -> SettingPage {
  page(
    "Lock Screen",
    IconName::Lock,
    vec![group(
      "Lock screen",
      vec![
        item(
          "Lock before suspend",
          "Lock the screen whenever the system suspends.",
          switch(
            |c| c.lockscreen.lock_before_suspend,
            |c, v| c.lockscreen.lock_before_suspend = v,
          ),
        ),
        item(
          "Blur",
          "Of the screen behind the lock, 0 for none.",
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
    "Screenshot",
    IconName::Camera,
    vec![group(
      "Saving",
      vec![
        item(
          "Directory",
          "Where screenshots are saved; may start with ~/.",
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
          "File name",
          "strftime pattern.",
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
    "Location & Weather",
    IconName::CloudSun,
    vec![
      group(
        "Location",
        vec![
          item(
            "City",
            "Looked up when no coordinates are set.",
            text(
              "location.city",
              |c| c.location.city.clone().unwrap_or_default(),
              |c, v| c.location.city = some(v),
            ),
          ),
          item(
            "Coordinates",
            "Latitude, longitude; used over the city.",
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
            "Locate automatically",
            "Ask GeoClue first.",
            switch(
              |c| c.location.auto_locate,
              |c, v| c.location.auto_locate = v,
            ),
          ),
        ],
      ),
      group(
        "Weather",
        vec![
          item(
            "Units",
            "For temperatures, wind and rain.",
            choice(
              &[(Units::Metric, "Metric"), (Units::Imperial, "Imperial")],
              |c| c.weather.units,
              |c, v| c.weather.units = v,
            ),
          ),
          item(
            "Refresh every",
            "Minutes.",
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
    "Privacy",
    IconName::Shield,
    vec![group(
      "Hidden apps",
      vec![
        item(
          "Microphone",
          "Case-insensitive regex against the app's name; matches are left out of the privacy widget, its log and OSD.",
          text("shell.privacy.mic", |c| c.shell.privacy.mic_filter_regex.clone(), |c, v| c.shell.privacy.mic_filter_regex = v),
        )
        .layout(Axis::Vertical),
        item("Camera", "Same, for camera access.", text("shell.privacy.cam", |c| c.shell.privacy.cam_filter_regex.clone(), |c, v| c.shell.privacy.cam_filter_regex = v))
          .layout(Axis::Vertical),
        item(
          "Screen",
          "Same, for screen sharing and recording.",
          text("shell.privacy.screen", |c| c.shell.privacy.screen_filter_regex.clone(), |c, v| c.shell.privacy.screen_filter_regex = v),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
}

fn system() -> SettingPage {
  page(
    "System",
    IconName::Cpu,
    vec![
      group(
        "Profile",
        vec![
          item(
            "Avatar",
            "Path, ~/ path or http(s) URL of the dashboard picture.",
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
        "Polling",
        vec![
          item(
            "System monitor",
            "Seconds between samples.",
            number(
              "system.monitor.poll_seconds",
              (1., 60., 1.),
              |c| c.system.monitor.poll_seconds as f64,
              |c, v| c.system.monitor.poll_seconds = v as u64,
            ),
          ),
          item(
            "Brightness",
            "Seconds between checks.",
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
        "Applies after a restart",
        vec![
          item(
            "ddcutil",
            "Brightness of external monitors.",
            switch(
              |c| c.brightness.enable_ddcutil,
              |c, v| c.brightness.enable_ddcutil = v,
            ),
          ),
          item(
            "Plugin directory",
            "Where plugins are loaded from; may start with ~/.",
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
