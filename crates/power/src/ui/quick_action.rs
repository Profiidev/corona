//! The dashboard's power profile tile

use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, QuickAction, Tile},
};
use corona_utils::error::spawn_logged;
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{
  PowerExt,
  ui::{icons::power_profile, page::PowerPanel},
};

fn profile_icon(active: Option<&str>) -> IconName {
  match active {
    Some("performance") => IconName::Gauge,
    Some("power-saver") => IconName::Leaf,
    _ => IconName::Scale,
  }
}

/// Lit for any profile but the default one
fn profile_active(active: Option<&str>) -> bool {
  active.is_some_and(|a| a != "balanced")
}

pub struct PowerProfileToggle;

impl QuickAction for PowerProfileToggle {
  const ID: &'static str = "power_profile";

  fn title() -> SharedString {
    t!("app.power.profile.title").into()
  }

  fn tile(cx: &App) -> Tile {
    let profiles = cx.power().profiles(cx);
    let active = profiles.map(|p| p.active.clone());
    // the next one, so a click cycles through them
    let next = profiles.and_then(|p| p.next().cloned());
    Tile {
      icon: profile_icon(active.as_deref()),
      status: match active.as_deref() {
        Some(name) => power_profile(name).1.into(),
        None => t!("app.common.unavailable").into(),
      },
      active: profile_active(active.as_deref()),
      toggle: next.map(|next| {
        Box::new(move |cx: &mut App| {
          let task = cx.power().set_profile(next.clone());
          spawn_logged(cx, task);
        }) as _
      }),
    }
  }

  fn page() -> Option<&'static str> {
    Some(PowerPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let profiles = cx.power().profiles.clone();
    vec![cx.observe(&profiles, move |_, cx| changed(cx))]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn profile() {
    assert!(!profile_active(None));
    assert!(!profile_active(Some("balanced")));
    assert!(profile_active(Some("performance")));
    assert!(profile_active(Some("power-saver")));
    assert_eq!(profile_icon(Some("performance")), IconName::Gauge);
    assert_eq!(profile_icon(Some("power-saver")), IconName::Leaf);
    assert_eq!(profile_icon(Some("balanced")), IconName::Scale);
    assert_eq!(profile_icon(None), IconName::Scale);
  }
}
