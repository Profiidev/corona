//! The screenshot page: where and under which name they are saved, and the screen ruler

use std::path::PathBuf;

use corona_components::settings_fields::{number, optional_text, switch, text};
use corona_settings::{SettingsExt, group, item, page, settings_page};
use gpui_kit::{App, Axis, assets::IconName, component::setting::SettingPage};
use rust_i18n::t;

settings_page!(ScreenshotSettings, "screenshot", 120, |cx| screenshot());

pub(super) fn register(cx: &mut App) {
  cx.settings_mut().register::<ScreenshotSettings>();
}

const FPS_RANGE: (f64, f64, f64) = (0., 500., 5.);

/// The live fps cap as a field: 0 stands for unset, the monitor's refresh rate
fn fps_field(fps: Option<u32>) -> f64 {
  f64::from(fps.unwrap_or(0))
}

fn fps_value(v: f64) -> Option<u32> {
  let v = v.clamp(FPS_RANGE.0, FPS_RANGE.1) as u32;
  (v > 0).then_some(v)
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
          item(
            t!("app.settings.screenshot.live.title"),
            t!("app.settings.screenshot.live.description"),
            switch(|c| c.screenshot.live, |c, v| c.screenshot.live = v),
          ),
        ],
      ),
      group(
        t!("app.settings.screenshot.groups.color_picker"),
        vec![
          item(
            t!("app.settings.screenshot.live.title"),
            t!("app.settings.screenshot.live.description"),
            switch(|c| c.color_picker.live, |c, v| c.color_picker.live = v),
          ),
          item(
            t!("app.settings.screenshot.live_fps.title"),
            t!("app.settings.screenshot.live_fps.description"),
            number(
              "color_picker.live_fps",
              FPS_RANGE,
              |c| fps_field(c.color_picker.live_fps),
              |c, v| c.color_picker.live_fps = fps_value(v),
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.screenshot.groups.ruler"),
        vec![
          item(
            t!("app.settings.screenshot.ruler_tolerance.title"),
            t!("app.settings.screenshot.ruler_tolerance.description"),
            number(
              "ruler.tolerance",
              (0., 255., 5.),
              |c| f64::from(c.ruler.tolerance),
              |c, v| c.ruler.tolerance = v.clamp(0., 255.) as u8,
            ),
          ),
          item(
            t!("app.settings.screenshot.live.title"),
            t!("app.settings.screenshot.live.description"),
            switch(|c| c.ruler.live, |c, v| c.ruler.live = v),
          ),
          item(
            t!("app.settings.screenshot.live_fps.title"),
            t!("app.settings.screenshot.live_fps.description"),
            number(
              "ruler.live_fps",
              FPS_RANGE,
              |c| fps_field(c.ruler.live_fps),
              |c, v| c.ruler.live_fps = fps_value(v),
            ),
          ),
        ],
      ),
    ],
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fps_zero_is_unset() {
    assert_eq!(fps_value(0.), None);
    assert_eq!(fps_value(-3.), None);
    assert_eq!(fps_value(30.), Some(30));
    assert_eq!(fps_value(9999.), Some(500));
    assert_eq!(fps_field(None), 0.);
    assert_eq!(fps_field(Some(60)), 60.);
  }
}
