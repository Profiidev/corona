//! The dashboard's stay awake tile

use corona_surface::dashboard::{Changed, QuickAction, Tile, on_off};
use gpui_kit::{App, BorrowAppContext, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use corona_idle::{Idle, IdleExt};

pub struct StayAwakeToggle;

impl QuickAction for StayAwakeToggle {
  const ID: &'static str = "stay_awake";

  fn title() -> SharedString {
    t!("app.dashboard.stay_awake").into()
  }

  fn tile(cx: &App) -> Tile {
    let on = cx.idle().stay_awake(cx);
    Tile {
      icon: IconName::Coffee,
      status: on_off(on),
      active: on,
      toggle: Some(Box::new(move |cx| {
        cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(!on, None, cx))
      })),
    }
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let stay_awake = cx.idle().stay_awake.clone();
    vec![cx.observe(&stay_awake, move |_, cx| changed(cx))]
  }
}
