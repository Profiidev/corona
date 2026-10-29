use crate::control_center::dashboard::DashboardPanel;
use crate::control_center::{BrightnessPanel, NetworkPanel, NotificationsPanel, PowerPanel};
use crate::icons::power_profile;
use corona_bluez::BluetoothExt;
use corona_brightness::BrightnessExt;
use corona_components::assets::toggle_mode;
use corona_config::ConfigProvider;
use corona_idle::{Idle, IdleExt};
use corona_network_manager::{NetworkManagerExt, WifiStatus};
use corona_notifications::NotificationsExt;
use corona_power::PowerExt;
use corona_surface::{
  control_center::{ControlCenter, ControlCenterPage},
  dashboard::{Changed, DashboardExt, DashboardRegistry, QuickAction, QuickActionEntry, Tile},
};
use corona_utils::error::{ErrorLogExt, spawn_logged};
use gpui_kit::{
  App, BorrowAppContext, ClickEvent, Div, ElementId, InteractiveElement, MouseButton,
  ParentElement, SharedString, Styled, Subscription, Window,
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
use rust_i18n::t;

const COLUMNS: usize = 3;

type Action = Box<dyn Fn(&mut App)>;

struct Toggle {
  icon: IconName,
  status: String,
  active: bool,
  on_click: Option<Action>,
}

/// A quick action whose tile `$build` makes, re-rendered when the entities
/// `$observe` names notify
macro_rules! action {
  (
    $name:ident, $id:literal, $title:literal, $build:ident, $page:expr,
    |$cx:ident| [$($entity:expr),* $(,)?]
  ) => {
    pub struct $name;

    impl QuickAction for $name {
      const ID: &'static str = $id;

      fn title() -> SharedString {
        t!($title).into()
      }

      fn tile(cx: &App) -> Tile {
        let toggle = $build(cx);
        Tile {
          icon: toggle.icon,
          status: toggle.status.into(),
          active: toggle.active,
          toggle: toggle.on_click,
        }
      }

      fn page() -> Option<&'static str> {
        $page
      }

      #[allow(unused_variables)]
      fn observe(changed: Changed, $cx: &mut App) -> Vec<Subscription> {
        vec![$({
          let (entity, changed) = ($entity.clone(), changed.clone());
          $cx.observe(&entity, move |_, cx| changed(cx))
        }),*]
      }
    }
  };
}

action!(
  Wifi,
  "wifi",
  "app.dashboard.wifi",
  wifi,
  Some(NetworkPanel::ID),
  |cx| [
    cx.network_manager().wifi_enabled,
    cx.network_manager().wifi_networks,
  ]
);
action!(
  Dnd,
  "dnd",
  "app.dashboard.dnd",
  dnd,
  Some(NotificationsPanel::ID),
  |cx| [cx.notifications().do_not_disturb,]
);
// a theme change redraws every window anyway
action!(
  DarkMode,
  "dark_mode",
  "app.dashboard.dark_mode",
  dark_mode,
  None,
  |cx| []
);
action!(
  StayAwake,
  "stay_awake",
  "app.dashboard.stay_awake",
  stay_awake,
  None,
  |cx| [cx.idle().stay_awake,]
);
action!(
  Nightlight,
  "nightlight",
  "app.brightness.nightlight",
  nightlight,
  Some(BrightnessPanel::ID),
  |cx| [cx.brightness().nightlight,]
);
action!(
  Airplane,
  "airplane",
  "app.dashboard.airplane",
  airplane,
  Some(NetworkPanel::ID),
  |cx| [cx.network_manager().wifi_enabled, cx.bluetooth().adapter,]
);
action!(
  PowerProfile,
  "power_profile",
  "app.power.profile.title",
  profile,
  Some(PowerPanel::ID),
  |cx| [cx.power().profiles,]
);

/// Registers every tile, in the order the settings offer them
pub(crate) fn register(cx: &mut App) {
  cx.dashboard_mut()
    .quick_action::<Wifi>()
    .quick_action::<Dnd>()
    .quick_action::<DarkMode>()
    .quick_action::<StayAwake>()
    .quick_action::<Nightlight>()
    .quick_action::<Airplane>()
    .quick_action::<PowerProfile>();
}

/// The configured tiles, None for none
pub(super) fn toggles(cx: &mut gpui_kit::Context<DashboardPanel>) -> Option<Div> {
  let registry = cx.dashboard();
  let (actions, _) = DashboardRegistry::pick(
    registry.quick_actions(),
    &cx.config().control_center.quick_actions,
    |a| a.id,
  );
  if actions.is_empty() {
    return None;
  }
  let tiles: Vec<_> = (actions.into_iter())
    .map(|action| tile(action, (action.tile)(cx)))
    .collect();

  let mut rows = div().flex().flex_col().gap(px(6.));
  let mut tiles = tiles.into_iter().peekable();
  while tiles.peek().is_some() {
    let row: Vec<_> = tiles
      .by_ref()
      .take(COLUMNS)
      .map(|tile| div().flex_1().min_w_0().child(tile))
      .collect();
    rows = rows.child(div().flex().gap(px(6.)).children(row));
  }
  Some(rows)
}

fn tile(action: &QuickActionEntry, tile: Tile) -> Button {
  let page = action.page;
  let enabled = tile.toggle.is_some();

  Button::new(ElementId::Name(format!("toggle-{}", action.id).into()))
    .with_variant(match tile.active {
      true => ButtonVariant::Primary,
      false => ButtonVariant::Default,
    })
    .icon(Icon::new(tile.icon).small())
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
        .child(div().text_sm().truncate().child((action.title)()))
        .child(div().text_xs().opacity(0.7).truncate().child(tile.status)),
    )
    .when_some(tile.toggle, |b, toggle| {
      b.cursor_pointer()
        .on_click(move |_: &ClickEvent, _, cx| toggle(cx))
    })
    .when_some(page, |b, page| {
      b.on_mouse_down(MouseButton::Right, move |_, window: &mut Window, cx| {
        ControlCenter::navigate_id(page, window, cx)
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
pub(crate) fn airplane_on(wifi: bool, bluetooth: Option<bool>) -> bool {
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
    icon: if enabled {
      IconName::Wifi
    } else {
      IconName::WifiOff
    },
    status: wifi_status(enabled, connected),
    active: enabled,
    on_click: network.wifi_supported(cx).then(|| {
      Box::new(move |cx: &mut App| {
        let task = cx.network_manager().set_wifi_enabled(!enabled);
        spawn_logged(cx, task);
      }) as Action
    }),
  }
}

fn dnd(cx: &App) -> Toggle {
  let enabled = cx.notifications().do_not_disturb(cx);
  Toggle {
    icon: if enabled {
      IconName::BellOff
    } else {
      IconName::Bell
    },
    status: on_off(enabled),
    active: enabled,
    on_click: Some(Box::new(move |cx| {
      cx.notifications().clone().set_do_not_disturb(!enabled, cx)
    })),
  }
}

fn dark_mode(cx: &App) -> Toggle {
  let dark = cx.theme().is_dark();
  Toggle {
    icon: if dark { IconName::Moon } else { IconName::Sun },
    status: on_off(dark),
    active: dark,
    on_click: Some(Box::new(toggle_mode)),
  }
}

fn profile(cx: &App) -> Toggle {
  let profiles = cx.power().profiles(cx);
  let active = profiles.map(|p| p.active.clone());
  // the next one, so a click cycles through them
  let next = profiles.and_then(|p| p.next().cloned());
  Toggle {
    icon: profile_icon(active.as_deref()),
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
  }
}

/// Every radio off for on, back on for off
pub(crate) fn set_airplane(on: bool, cx: &mut App) {
  let task = cx.network_manager().set_wifi_enabled(!on);
  spawn_logged(cx, task);
  if cx.bluetooth().adapter(cx).is_some() {
    let task = cx.bluetooth().set_powered(!on, cx);
    spawn_logged(cx, task);
  }
}

/// Whether every radio is off
pub(crate) fn airplane_state(cx: &App) -> bool {
  airplane_on(
    cx.network_manager().wifi_enabled(cx),
    cx.bluetooth().adapter(cx).map(|a| a.powered),
  )
}

fn airplane(cx: &App) -> Toggle {
  let wifi = cx.network_manager().wifi_enabled(cx);
  let bluetooth = cx.bluetooth().adapter(cx).map(|a| a.powered);
  let on = airplane_on(wifi, bluetooth);
  Toggle {
    icon: IconName::Plane,
    status: on_off(on),
    active: on,
    on_click: Some(Box::new(move |cx| set_airplane(!on, cx))),
  }
}

fn stay_awake(cx: &App) -> Toggle {
  let on = cx.idle().stay_awake(cx);
  Toggle {
    icon: IconName::Coffee,
    status: on_off(on),
    active: on,
    on_click: Some(Box::new(move |cx| {
      cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(!on, None, cx))
    })),
  }
}

fn nightlight(cx: &App) -> Toggle {
  let state = *cx.brightness().nightlight(cx);
  Toggle {
    icon: IconName::Moon,
    status: match state.available {
      true => on_off(state.on),
      false => t!("app.common.unavailable").into(),
    },
    active: state.on,
    on_click: state.available.then(|| -> Action {
      Box::new(move |cx| {
        let _ = cx.brightness().set_nightlight(!state.on, cx).log_err();
      })
    }),
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
