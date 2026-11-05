//! The dashboard's Bluetooth tile

use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, QuickAction, Tile, on_off},
};
use corona_utils::error::spawn_logged;
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{BluetoothExt, ui::page::BluetoothPanel};

/// `powered` is None without an adapter
fn status(powered: Option<bool>, connected: usize) -> String {
  match (powered, connected) {
    (None, _) => t!("app.common.unavailable").into(),
    (Some(true), 1) => t!("app.dashboard.devices.one").into(),
    (Some(true), n) if n > 1 => t!("app.dashboard.devices.other", count = n).into(),
    (Some(on), _) => on_off(on).into(),
  }
}

pub struct BluetoothToggle;

impl QuickAction for BluetoothToggle {
  const ID: &'static str = "bluetooth";

  fn title() -> SharedString {
    t!("app.dashboard.bluetooth").into()
  }

  fn tile(cx: &App) -> Tile {
    let bluetooth = cx.bluetooth();
    let powered = bluetooth.adapter(cx).map(|a| a.powered);
    let connected = bluetooth
      .list_devices(cx)
      .iter()
      .filter(|d| d.connected)
      .count();
    Tile {
      icon: match powered {
        Some(true) => IconName::Bluetooth,
        _ => IconName::BluetoothOff,
      },
      status: status(powered, connected).into(),
      active: powered == Some(true),
      toggle: powered.map(|powered| {
        Box::new(move |cx: &mut App| {
          let task = cx.bluetooth().set_powered(!powered, cx);
          spawn_logged(cx, task);
        }) as _
      }),
    }
  }

  fn page() -> Option<&'static str> {
    Some(BluetoothPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let bluetooth = cx.bluetooth().clone();
    let adapter = changed.clone();
    vec![
      cx.observe(&bluetooth.adapter, move |_, cx| adapter(cx)),
      cx.observe(&bluetooth.devices, move |_, cx| changed(cx)),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn statuses() {
    let unavailable = status(None, 3);
    assert_ne!(unavailable, on_off(false).to_string());
    assert_eq!(status(None, 0), unavailable);
    assert_eq!(status(Some(true), 0), on_off(true).to_string());
    assert_eq!(status(Some(false), 2), on_off(false).to_string());
    let one = status(Some(true), 1);
    let several = status(Some(true), 3);
    assert_ne!(one, on_off(true).to_string());
    assert_ne!(one, several);
    assert!(several.contains('3'), "{several}");
  }
}
