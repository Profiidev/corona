//! The dashboard's brightness slider, on the focused display

use std::time::Duration;

use corona_compositor::CompositorExt;
use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, DashboardSlider},
};
use corona_utils::error::spawn_logged;
use gpui_kit::{App, Entity, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{BrightnessExt, Display, ui::page::BrightnessPanel};

/// the focused monitor's display, or the first usable one
pub fn display(cx: &App) -> Option<&Display> {
  let focused = &cx.compositor().active_monitor(cx).name;
  pick_display(cx.brightness().list_displays(cx), focused)
}

fn pick_display<'d>(displays: &'d [Display], focused: &str) -> Option<&'d Display> {
  let usable = || displays.iter().filter(|d| d.unavailable.is_none());
  usable()
    .find(|d| d.output.as_deref() == Some(focused))
    .or_else(|| usable().next())
}

fn observe<T: 'static>(entity: &Entity<T>, changed: Changed, cx: &mut App) -> Subscription {
  cx.observe(entity, move |_, cx| changed(cx))
}

pub struct Brightness;

impl DashboardSlider for Brightness {
  const ID: &'static str = "brightness";
  // each step is a DDC/CI or sysfs write
  const THROTTLE: Option<Duration> = Some(Duration::from_millis(100));

  fn title() -> SharedString {
    t!("app.dashboard.sliders.brightness").into()
  }

  fn icon(_: &App) -> IconName {
    IconName::Sun
  }

  fn value(cx: &App) -> Option<f32> {
    display(cx).map(|d| d.percent() / 100.)
  }

  fn set(value: f32, cx: &mut App) {
    let Some(display) = display(cx) else {
      return;
    };
    let raw = (value * display.max as f32).round() as u32;
    let task = cx.brightness().set_brightness(&display.id, raw, cx);
    spawn_logged(cx, task);
  }

  fn page() -> Option<&'static str> {
    Some(BrightnessPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let displays = cx.brightness().displays.clone();
    let monitor = cx.compositor().active_monitor.clone();
    vec![
      observe(&displays, changed.clone(), cx),
      observe(&monitor, changed, cx),
    ]
  }
}

#[cfg(test)]
mod tests {
  use crate::{DisplayKind, Unavailable};

  use super::*;

  fn display(id: &str, output: Option<&str>, unavailable: Option<Unavailable>) -> Display {
    Display {
      id: id.into(),
      output: output.map(Into::into),
      name: None,
      kind: DisplayKind::External,
      brightness: 50,
      max: 100,
      unavailable,
    }
  }

  fn picked(displays: &[Display], focused: &str) -> Option<String> {
    pick_display(displays, focused).map(|d| d.id.clone())
  }

  #[test]
  fn pick_display_prefers_focused() {
    let displays = [
      display("a", Some("DP-1"), None),
      display("b", Some("DP-2"), None),
    ];
    assert_eq!(picked(&displays, "DP-2").as_deref(), Some("b"));
    assert_eq!(picked(&displays, "HDMI-1").as_deref(), Some("a"));
  }

  #[test]
  fn pick_display_skips_unavailable() {
    let displays = [
      display("a", Some("DP-1"), Some(Unavailable::Detecting)),
      display("b", None, None),
      display("c", Some("DP-2"), Some(Unavailable::Failed)),
    ];
    // the focused one is unusable, so the first usable one
    assert_eq!(picked(&displays, "DP-2").as_deref(), Some("b"));
    assert_eq!(picked(&displays[..1], "DP-1"), None);
    assert_eq!(picked(&[], "DP-1"), None);
  }
}
