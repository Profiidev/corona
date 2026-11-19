//! The screenshot page: where and under which name they are saved, and the screen ruler

use std::path::PathBuf;

use corona_components::settings_fields::{number, optional_text, text};
use corona_settings::{SettingsExt, group, item, page, settings_page};
use gpui_kit::{App, Axis, assets::IconName, component::setting::SettingPage};
use rust_i18n::t;

settings_page!(ScreenshotSettings, "screenshot", 120, |cx| screenshot());

pub(super) fn register(cx: &mut App) {
  cx.settings_mut().register::<ScreenshotSettings>();
}

fn screenshot() -> SettingPage {
  page(
    t!("app.settings.screenshot.title"),
    IconName::Camera,
    vec![
      group(
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
      ),
      group(
        t!("app.settings.screenshot.groups.ruler"),
        vec![item(
          t!("app.settings.screenshot.ruler_tolerance.title"),
          t!("app.settings.screenshot.ruler_tolerance.description"),
          number(
            "ruler.tolerance",
            (0., 255., 5.),
            |c| f64::from(c.ruler.tolerance),
            |c, v| c.ruler.tolerance = v.clamp(0., 255.) as u8,
          ),
        )],
      ),
    ],
  )
}
