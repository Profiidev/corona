//! The dashboard's do not disturb tile

use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, QuickAction, Tile, on_off},
};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::panel::notifications::NotificationsPanel;
use corona_notifications::NotificationsExt;

pub struct DndToggle;

impl QuickAction for DndToggle {
  const ID: &'static str = "dnd";

  fn title() -> SharedString {
    t!("app.dashboard.dnd").into()
  }

  fn tile(cx: &App) -> Tile {
    let enabled = cx.notifications().do_not_disturb(cx);
    Tile {
      icon: if enabled {
        IconName::BellOff
      } else {
        IconName::Bell
      },
      status: on_off(enabled),
      active: enabled,
      toggle: Some(Box::new(move |cx| {
        cx.notifications().clone().set_do_not_disturb(!enabled, cx)
      })),
    }
  }

  fn page() -> Option<&'static str> {
    Some(NotificationsPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let dnd = cx.notifications().do_not_disturb.clone();
    vec![cx.observe(&dnd, move |_, cx| changed(cx))]
  }
}
