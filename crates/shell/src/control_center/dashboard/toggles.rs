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

use crate::control_center::{
  ControlCenter,
  dashboard::{DashboardPanel, spawn_logged},
  variants::ControlCenterType,
};

const COLUMNS: usize = 3;

type Action = Box<dyn Fn(&mut App)>;

struct Toggle {
  id: &'static str,
  icon: IconName,
  label: &'static str,
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
    true => "On",
    false => "Off",
  }
  .into()
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
    label: "Wi-Fi",
    status: connected
      .filter(|_| enabled)
      .unwrap_or_else(|| on_off(enabled)),
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
    label: "Bluetooth",
    status: match (powered, connected) {
      (None, _) => "Unavailable".into(),
      (Some(true), 1) => "1 device".into(),
      (Some(true), n) if n > 1 => format!("{n} devices"),
      (Some(on), _) => on_off(on),
    },
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
    label: "DND",
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
    label: "Dark mode",
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
    icon: match active.as_deref() {
      Some("performance") => IconName::Gauge,
      Some("power-saver") => IconName::Leaf,
      _ => IconName::Scale,
    },
    label: "Power profile",
    status: match active.as_deref() {
      Some("performance") => "Performance".into(),
      Some("power-saver") => "Power saver".into(),
      Some("balanced") => "Balanced".into(),
      Some(other) => other.to_string(),
      None => "Unavailable".into(),
    },
    active: active.as_deref().is_some_and(|a| a != "balanced"),
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
  let on = !wifi && bluetooth != Some(true);
  Toggle {
    id: "toggle-airplane",
    icon: IconName::Plane,
    label: "Airplane",
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
