//! The taskbar and window switcher pages

use corona_components::settings_fields::{number, slider, switch};
use corona_settings::{SettingsExt, group, item, page, settings_page};
use gpui_kit::{App, assets::IconName, component::setting::SettingPage};
use rust_i18n::t;

const OPACITY: (f32, f32, f32) = (0., 1., 0.05);

settings_page!(TaskbarSettings, "taskbar", 60, |cx| taskbar());

settings_page!(WindowSwitcherSettings, "window_switcher", 70, |cx| {
  window_switcher()
});

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<TaskbarSettings>()
    .register::<WindowSwitcherSettings>();
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
