use corona_bluez::BluetoothExt;
use corona_components::assets::toggle_mode;
use corona_network_manager::{NetworkManagerExt, WifiStatus};
use corona_notifications::NotificationsExt;
use corona_power::PowerExt;
use gpui_kit::{
  App, ClickEvent, Div, ElementId, InteractiveElement, MouseButton, ParentElement, Styled, Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};
use std::borrow::Cow;

use crate::control_center::{
  ControlCenter,
  dashboard::{DashboardPanel, spawn_logged},
  variants::ControlCenterType,
};
use crate::icons::power_profile;
use rust_i18n::t;

const COLUMNS: usize = 3;

type Action = Box<dyn Fn(&mut App)>;

struct Toggle {
  id: &'static str,
  icon: IconName,
  label: Cow<'static, str>,
  status: String,
  active: bool,
  on_click: Option<Action>,
  page: Option<ControlCenterType>,
}

pub(super) fn toggles(cx: &mut gpui_kit::Context<DashboardPanel>) -> Div {
  let tiles: Vec<Toggle> = [
    wifi(cx),
    bluetooth(cx),
    dnd(cx),
    dark_mode(cx),
    profile(cx),
    airplane(cx),
  ]
  .into_iter()
  .collect();

  let mut rows = div().flex().flex_col().gap(px(6.));
  let mut tiles = tiles.into_iter().peekable();
  while tiles.peek().is_some() {
    let row: Vec<_> = tiles
      .by_ref()
      .take(COLUMNS)
      .map(|toggle| div().flex_1().min_w_0().child(tile(toggle)))
      .collect();
    rows = rows.child(div().flex().gap(px(6.)).children(row));
  }
  rows
}

fn tile(toggle: Toggle) -> Button {
  let page = toggle.page;
  let enabled = toggle.on_click.is_some();

  Button::new(ElementId::Name(toggle.id.into()))
    .with_variant(match toggle.active {
      true => ButtonVariant::Primary,
      false => ButtonVariant::Default,
    })
    .icon(Icon::new(toggle.icon).small())
    .disabled(!enabled)
    .w_full()
    .h_auto()
    .py_1()
    .rounded_xl()
    .child(
      div()
        .flex_1()
        .flex()
        .flex_col()
        .min_w_0()
        .child(div().text_sm().truncate().child(toggle.label))
        .child(div().text_xs().opacity(0.7).truncate().child(toggle.status)),
    )
    .when_some(toggle.on_click, |b, on_click| {
      b.cursor_pointer()
        .on_click(move |_: &ClickEvent, _, cx| on_click(cx))
    })
    .when_some(page, |b, page| {
      b.on_mouse_down(MouseButton::Right, move |_, window: &mut Window, cx| {
        ControlCenter::navigate(page, window, cx)
      })
    })
}

fn on_off(on: bool) -> String {
  match on {
    true => t!("app.common.on"),
    false => t!("app.common.off"),
  }
  .into()
}

/// The connected network's name while wifi is on
fn wifi_status(enabled: bool, connected: Option<String>) -> String {
  connected
    .filter(|_| enabled)
    .unwrap_or_else(|| on_off(enabled))
}

/// `powered` is None without an adapter
fn bluetooth_status(powered: Option<bool>, connected: usize) -> String {
  match (powered, connected) {
    (None, _) => t!("app.common.unavailable").into(),
    (Some(true), 1) => t!("app.dashboard.devices.one").into(),
    (Some(true), n) if n > 1 => t!("app.dashboard.devices.other", count = n).into(),
    (Some(on), _) => on_off(on),
  }
}

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

/// Airplane mode is on while no radio is
fn airplane_on(wifi: bool, bluetooth: Option<bool>) -> bool {
  !wifi && bluetooth != Some(true)
}

fn wifi(cx: &App) -> Toggle {
  let network = cx.network_manager();
  let enabled = network.wifi_enabled(cx);
  let connected = network
    .list_wifi_networks(cx)
    .iter()
    .find(|n| n.status == WifiStatus::Connected)
    .map(|n| n.ssid.clone());
  Toggle {
    id: "toggle-wifi",
    icon: if enabled {
      IconName::Wifi
    } else {
      IconName::WifiOff
    },
    label: t!("app.dashboard.wifi"),
    status: wifi_status(enabled, connected),
    active: enabled,
    on_click: network.wifi_supported(cx).then(|| {
      Box::new(move |cx: &mut App| {
        let task = cx.network_manager().set_wifi_enabled(!enabled);
        spawn_logged(cx, task);
      }) as Action
    }),
    page: Some(ControlCenterType::Network),
  }
}

fn bluetooth(cx: &App) -> Toggle {
  let bluetooth = cx.bluetooth();
  let powered = bluetooth.adapter(cx).map(|a| a.powered);
  let connected = bluetooth
    .list_devices(cx)
    .iter()
    .filter(|d| d.connected)
    .count();
  Toggle {
    id: "toggle-bluetooth",
    icon: match powered {
      Some(true) => IconName::Bluetooth,
      _ => IconName::BluetoothOff,
    },
    label: t!("app.dashboard.bluetooth"),
    status: bluetooth_status(powered, connected),
    active: powered == Some(true),
    on_click: powered.map(|powered| {
      Box::new(move |cx: &mut App| {
        let task = cx.bluetooth().set_powered(!powered, cx);
        spawn_logged(cx, task);
      }) as Action
    }),
    page: Some(ControlCenterType::Bluetooth),
  }
}

fn dnd(cx: &App) -> Toggle {
  let enabled = cx.notifications().do_not_disturb(cx);
  Toggle {
    id: "toggle-dnd",
    icon: if enabled {
      IconName::BellOff
    } else {
      IconName::Bell
    },
    label: t!("app.dashboard.dnd"),
    status: on_off(enabled),
    active: enabled,
    on_click: Some(Box::new(move |cx| {
      cx.notifications().clone().set_do_not_disturb(!enabled, cx)
    })),
    page: Some(ControlCenterType::Notifications),
  }
}

fn dark_mode(cx: &App) -> Toggle {
  let dark = cx.theme().is_dark();
  Toggle {
    id: "toggle-dark-mode",
    icon: if dark { IconName::Moon } else { IconName::Sun },
    label: t!("app.dashboard.dark_mode"),
    status: on_off(dark),
    active: dark,
    on_click: Some(Box::new(toggle_mode)),
    page: None,
  }
}

fn profile(cx: &App) -> Toggle {
  let profiles = cx.power().profiles(cx);
  let active = profiles.map(|p| p.active.clone());
  // the next one, so a click cycles through them
  let next = profiles.and_then(|p| p.next().cloned());
  Toggle {
    id: "toggle-profile",
    icon: profile_icon(active.as_deref()),
    label: t!("app.power.profile.title"),
    status: match active.as_deref() {
      Some(name) => power_profile(name).1.into(),
      None => t!("app.common.unavailable").into(),
    },
    active: profile_active(active.as_deref()),
    on_click: next.map(|next| {
      Box::new(move |cx: &mut App| {
        let task = cx.power().set_profile(next.clone());
        spawn_logged(cx, task);
      }) as Action
    }),
    page: Some(ControlCenterType::Power),
  }
}

fn airplane(cx: &App) -> Toggle {
  let wifi = cx.network_manager().wifi_enabled(cx);
  let bluetooth = cx.bluetooth().adapter(cx).map(|a| a.powered);
  let on = airplane_on(wifi, bluetooth);
  Toggle {
    id: "toggle-airplane",
    icon: IconName::Plane,
    label: t!("app.dashboard.airplane"),
    status: on_off(on),
    active: on,
    on_click: Some(Box::new(move |cx| {
      let task = cx.network_manager().set_wifi_enabled(on);
      spawn_logged(cx, task);
      if bluetooth.is_some() {
        let task = cx.bluetooth().set_powered(on, cx);
        spawn_logged(cx, task);
      }
    })),
    page: Some(ControlCenterType::Network),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn on_off_labels() {
    assert_eq!(super::on_off(true), "On");
    assert_eq!(super::on_off(false), "Off");
  }

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
  fn bluetooth_status() {
    let unavailable = super::bluetooth_status(None, 3);
    assert_ne!(unavailable, on_off(false));
    assert_eq!(super::bluetooth_status(None, 0), unavailable);
    assert_eq!(super::bluetooth_status(Some(true), 0), on_off(true));
    assert_eq!(super::bluetooth_status(Some(false), 2), on_off(false));
    let one = super::bluetooth_status(Some(true), 1);
    let several = super::bluetooth_status(Some(true), 3);
    assert_ne!(one, on_off(true));
    assert_ne!(one, several);
    assert!(several.contains('3'), "{several}");
  }

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

  #[test]
  fn airplane_on() {
    assert!(super::airplane_on(false, None));
    assert!(super::airplane_on(false, Some(false)));
    assert!(!super::airplane_on(false, Some(true)));
    assert!(!super::airplane_on(true, None));
    assert!(!super::airplane_on(true, Some(false)));
  }
}
