use corona_brightness::BrightnessExt;
use gpui_kit::{App, assets::IconName};

use crate::{
  control_center::brightness::display_name,
  osds::{
    on_change,
    view::{LevelOsd, show},
  },
};

pub fn init(cx: &mut App) {
  let displays = cx.brightness().displays.clone();
  on_change(
    &displays,
    cx,
    |cx| {
      let levels: Vec<(String, String, u32)> = cx
        .brightness()
        .list_displays(cx)
        .iter()
        .filter(|d| d.unavailable.is_none())
        .map(|d| (d.id.clone(), display_name(d), d.percent().round() as u32))
        .collect();
      Some(levels)
    },
    |prev, next, cx| {
      let changed = next.iter().find(|(id, _, percent)| {
        prev
          .iter()
          .any(|(prev_id, _, prev_percent)| prev_id == id && prev_percent != percent)
      });
      if let Some((_, name, percent)) = changed {
        let icon = if *percent < 50 {
          IconName::SunDim
        } else {
          IconName::Sun
        };
        show(
          |k| k.brightness,
          LevelOsd {
            icon,
            label: name.clone().into(),
            percent: *percent as f32,
            muted: false,
          },
          cx,
        );
      }
    },
  );
}
