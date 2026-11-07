//! The dashboard's Wi-Fi and airplane mode tiles, and airplane mode itself

use corona_bluez::BluetoothExt;
use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, QuickAction, Tile, on_off},
};
use corona_utils::error::spawn_logged;
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::panel::network::NetworkPanel;
use corona_network_manager::{NetworkManagerExt, WifiStatus};

/// The connected network's name while wifi is on
fn wifi_status(enabled: bool, connected: Option<String>) -> SharedString {
  connected
    .filter(|_| enabled)
    .map_or_else(|| on_off(enabled), Into::into)
}

/// Airplane mode is on while no radio is
pub fn airplane_on(wifi: bool, bluetooth: Option<bool>) -> bool {
  !wifi && bluetooth != Some(true)
}

/// Every radio off for on, back on for off
pub fn set_airplane(on: bool, cx: &mut App) {
  let task = cx.network_manager().set_wifi_enabled(!on);
  spawn_logged(cx, task);
  if cx.bluetooth().adapter(cx).is_some() {
    let task = cx.bluetooth().set_powered(!on, cx);
    spawn_logged(cx, task);
  }
}

/// Whether every radio is off
pub fn airplane_state(cx: &App) -> bool {
  airplane_on(
    cx.network_manager().wifi_enabled(cx),
    cx.bluetooth().adapter(cx).map(|a| a.powered),
  )
}

pub struct WifiToggle;

impl QuickAction for WifiToggle {
  const ID: &'static str = "wifi";

  fn title() -> SharedString {
    t!("app.dashboard.wifi").into()
  }

  fn tile(cx: &App) -> Tile {
    let network = cx.network_manager();
    let enabled = network.wifi_enabled(cx);
    let connected = network
      .list_wifi_networks(cx)
      .iter()
      .find(|n| n.status == WifiStatus::Connected)
      .map(|n| n.ssid.clone());
    Tile {
      icon: if enabled {
        IconName::Wifi
      } else {
        IconName::WifiOff
      },
      status: wifi_status(enabled, connected),
      active: enabled,
      toggle: network.wifi_supported(cx).then(|| {
        Box::new(move |cx: &mut App| {
          let task = cx.network_manager().set_wifi_enabled(!enabled);
          spawn_logged(cx, task);
        }) as _
      }),
    }
  }

  fn page() -> Option<&'static str> {
    Some(NetworkPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let network = cx.network_manager().clone();
    let enabled = changed.clone();
    vec![
      cx.observe(&network.wifi_enabled, move |_, cx| enabled(cx)),
      cx.observe(&network.wifi_networks, move |_, cx| changed(cx)),
    ]
  }
}

pub struct AirplaneToggle;

impl QuickAction for AirplaneToggle {
  const ID: &'static str = "airplane";

  fn title() -> SharedString {
    t!("app.dashboard.airplane").into()
  }

  fn tile(cx: &App) -> Tile {
    let on = airplane_state(cx);
    Tile {
      icon: IconName::Plane,
      status: on_off(on),
      active: on,
      toggle: Some(Box::new(move |cx| set_airplane(!on, cx))),
    }
  }

  fn page() -> Option<&'static str> {
    Some(NetworkPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let wifi = cx.network_manager().wifi_enabled.clone();
    let adapter = cx.bluetooth().adapter.clone();
    let radio = changed.clone();
    vec![
      cx.observe(&wifi, move |_, cx| radio(cx)),
      cx.observe(&adapter, move |_, cx| changed(cx)),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wifi_status() {
    assert_eq!(super::wifi_status(true, Some("Home".into())), "Home");
    assert_eq!(super::wifi_status(true, None), on_off(true));
    // a stale connection does not show while wifi is off
    assert_eq!(
      super::wifi_status(false, Some("Home".into())),
      on_off(false)
    );
  }

  #[test]
  fn airplane_on() {
    assert!(super::airplane_on(false, None));
    assert!(super::airplane_on(false, Some(false)));
    assert!(!super::airplane_on(false, Some(true)));
    assert!(!super::airplane_on(true, None));
    assert!(!super::airplane_on(true, Some(false)));
  }
}
