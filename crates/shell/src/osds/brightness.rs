use corona_brightness::BrightnessExt;
use gpui_kit::{App, assets::IconName};

use crate::{
  control_center::brightness::display_name,
  osds::{
    on_change,
    view::{LevelOsd, show},
  },
};

type Level = (String, String, u32);

/// The first display whose level changed; new ones are not a change
fn changed<'a>(prev: &[Level], next: &'a [Level]) -> Option<&'a Level> {
  next.iter().find(|(id, _, percent)| {
    prev
      .iter()
      .any(|(prev_id, _, prev_percent)| prev_id == id && prev_percent != percent)
  })
}

pub fn init(cx: &mut App) {
  let displays = cx.brightness().displays.clone();
  on_change(
    &displays,
    cx,
    |cx| {
      let levels: Vec<Level> = cx
        .brightness()
        .list_displays(cx)
        .iter()
        .filter(|d| d.unavailable.is_none())
        .map(|d| (d.id.clone(), display_name(d), d.percent().round() as u32))
        .collect();
      Some(levels)
    },
    |prev, next, cx| {
      if let Some((_, name, percent)) = changed(prev, next) {
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

#[cfg(test)]
mod tests {
  use super::*;

  fn l(id: &str, percent: u32) -> Level {
    (id.into(), id.to_uppercase(), percent)
  }

  #[test]
  fn finds_changed_display() {
    let prev = [l("a", 10), l("b", 20)];
    assert_eq!(changed(&prev, &[l("a", 10), l("b", 30)]), Some(&l("b", 30)));
    assert_eq!(changed(&prev, &[l("a", 15), l("b", 30)]), Some(&l("a", 15)));
    assert_eq!(changed(&prev, &prev), None);
  }

  #[test]
  fn added_or_removed_is_no_change() {
    let prev = [l("a", 10)];
    assert_eq!(changed(&prev, &[l("a", 10), l("new", 50)]), None);
    assert_eq!(changed(&prev, &[]), None);
    assert_eq!(changed(&[], &prev), None);
  }
}
