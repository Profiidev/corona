//! The dashboard's night light tile

use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, QuickAction, Tile, on_off},
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::panel::brightness::BrightnessPanel;
use corona_brightness::BrightnessExt;

pub struct NightlightToggle;

impl QuickAction for NightlightToggle {
  const ID: &'static str = "nightlight";

  fn title() -> SharedString {
    t!("app.brightness.nightlight").into()
  }

  fn tile(cx: &App) -> Tile {
    let state = *cx.brightness().nightlight(cx);
    Tile {
      icon: IconName::Moon,
      status: match state.available {
        true => on_off(state.on),
        false => t!("app.common.unavailable").into(),
      },
      active: state.on,
      toggle: state.available.then(|| {
        Box::new(move |cx: &mut App| {
          let _ = cx.brightness().set_nightlight(!state.on, cx).log_err();
        }) as _
      }),
    }
  }

  fn page() -> Option<&'static str> {
    Some(BrightnessPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let nightlight = cx.brightness().nightlight.clone();
    vec![cx.observe(&nightlight, move |_, cx| changed(cx))]
  }
}
