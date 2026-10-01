use corona_components::async_listener::AsyncListenerExt;
use corona_network_manager::{
  ActiveConnectionState, DeviceState, Interface, InterfaceType, NetworkManagerExt,
  NmConnectivityState, Vpn, VpnKind, WifiNetwork, WifiStatus,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
  Styled, Window,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    ActiveTheme, Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    empty::Empty,
    scroll::ScrollableElement,
    spinner::Spinner,
    switch::Switch,
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::ControlCenterPanel;

fn address(i: &Interface) -> String {
  if let Some(addr) = i.ip {
    format!("{}/{}", addr.address, addr.prefix)
  } else {
    "No address".to_string()
  }
}

#[derive(Debug, PartialEq, Eq)]
enum LoadingState {
  Idle,
  Loading,
  Error,
}

impl<T, E> From<Result<T, E>> for LoadingState {
  fn from(result: Result<T, E>) -> Self {
    if result.is_ok() {
      Self::Idle
    } else {
      Self::Error
    }
  }
}

pub struct NetworkPanel {
  connectivity_checking: LoadingState,
  wifi_rescanning: LoadingState,
}

impl ControlCenterPanel for NetworkPanel {
  fn init(_window: &mut Window, _cx: &mut Context<'_, Self>) -> Self {
    Self {
      connectivity_checking: LoadingState::Idle,
      wifi_rescanning: LoadingState::Idle,
    }
  }
}

impl NetworkPanel {
  fn status(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let primary = cx.network_manager().primary_interface(cx);
    let state = cx.network_manager().connectivity(cx);
    let connectivity_check_enabled = cx.network_manager().connectivity_check(cx).is_some();

    let icon = match primary {
      None => IconName::GlobeOff,
      Some(i) if i.kind == InterfaceType::Wired => IconName::EthernetPort,
      Some(_) => IconName::Wifi,
    };

    let (label, variant) = match state {
      NmConnectivityState::Full => ("Online", TagVariant::Success),
      NmConnectivityState::Portal => ("Sign-in required", TagVariant::Warning),
      NmConnectivityState::Loss => ("Limited", TagVariant::Warning),
      NmConnectivityState::None => ("Offline", TagVariant::Danger),
      NmConnectivityState::Unknown => ("Unknown", TagVariant::Secondary),
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(Icon::new(icon))
          .child(
            div()
              .flex()
              .text_sm()
              .font_bold()
              .child(primary.map_or("Disconnected".to_string(), |i| i.name.clone())),
          )
          .child(
            div()
              .flex()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(primary.map_or("No active connection".to_string(), address)),
          )
          .child(
            Button::new("connectivity-check")
              .small()
              .ml_auto()
              .cursor_pointer()
              .tooltip("Recheck")
              .disabled(!connectivity_check_enabled)
              .loading(self.connectivity_checking == LoadingState::Loading)
              .when_else(
                self.connectivity_checking == LoadingState::Error,
                |b| {
                  b.with_variant(ButtonVariant::Danger)
                    .icon(IconName::RotateCw)
                },
                |b| b.icon(IconName::RefreshCw),
              )
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.connectivity_checking = LoadingState::Loading;
                  cx.network_manager().check_connectivity()
                },
                |this, result, _| this.connectivity_checking = result.log_err().into(),
              )),
          )
          .child(Tag::new().small().with_variant(variant).child(label)),
      )
      .when(!connectivity_check_enabled, |d| {
        d.child(
          div()
            .flex()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child("Connectivity check is off, captive portals aren't detected"),
        )
      })
      .when(state == NmConnectivityState::Portal, |d| {
        d.child(
          div()
            .flex()
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child("Sign in to the network to get online"),
            )
            .child(
              Button::new("open-portal")
                .small()
                .label("Open")
                .cursor_pointer()
                .ml_auto()
                .icon(IconName::ExternalLink)
                .on_click(cx.listener(|_, _, _, cx| {
                  cx.network_manager().open_portal(cx);
                  cx.notify();
                })),
            ),
        )
      })
  }

  fn wifi(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let device = cx.network_manager().primary_wifi(cx);
    let enabled = cx.network_manager().wifi_enabled(cx);
    let supported = cx.network_manager().wifi_supported(cx);
    let networks = cx.network_manager().list_wifi_networks(cx);

    let placeholder_text = if !supported {
      Some("Wi-Fi not supported")
    } else if device.is_none() {
      Some("Wi-Fi is disabled")
    } else if networks.is_empty() {
      Some("No networks found")
    } else {
      None
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .flex_1()
      .min_h(px(160.))
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("Wi-Fi"))
          .child(
            Button::new("join-hidden-network")
              .icon(IconName::Plus)
              .disabled(device.is_none())
              .small()
              .ml_auto()
              .cursor_pointer()
              .tooltip("Join hidden network"),
          )
          .child(
            Button::new("wifi-rescan")
              .icon(IconName::RefreshCw)
              .disabled(device.is_none())
              .small()
              .cursor_pointer()
              .tooltip("Scan for networks")
              .loading(self.wifi_rescanning == LoadingState::Loading)
              .when_else(
                self.wifi_rescanning == LoadingState::Error,
                |b| {
                  b.with_variant(ButtonVariant::Danger)
                    .icon(IconName::RotateCw)
                },
                |b| b.icon(IconName::RefreshCw),
              )
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.wifi_rescanning = LoadingState::Loading;
                  cx.network_manager().rescan(cx)
                },
                |this, result, _| this.wifi_rescanning = result.log_err().into(),
              )),
          )
          .child(
            Switch::new("wifi-enabled")
              .checked(enabled)
              .disabled(!supported)
              // notify resets the switch if toggling failed
              .on_change(cx.async_listener(
                |_, checked, _, cx| cx.network_manager().set_wifi_enabled(*checked),
                |_, result, _| {
                  result.log_err().ok();
                },
              )),
          ),
      )
      .when_some(placeholder_text, |d, text| {
        d.child(
          div()
            .flex()
            .justify_center()
            .p_2()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(text),
        )
      })
      .when_none(&placeholder_text, |d| {
        d.child(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .children(networks.iter().map(|n| self.network(theme, n, device, cx))),
        )
      })
  }

  fn network(
    &self,
    theme: &Theme,
    network: &WifiNetwork,
    device: Option<&Interface>,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let interface = device.map(|d| d.name.clone()).unwrap_or_default();
    let signal_icon = match network.strength {
      0..25 => IconName::WifiZero,
      25..50 => IconName::WifiLow,
      50..75 => IconName::WifiHigh,
      75.. => IconName::Wifi,
    };

    div()
      .id(format!("wifi-network-{}", network.ssid))
      .flex()
      .gap_2()
      .p_2()
      .w_full()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(Icon::new(signal_icon).small())
      .child(
        div()
          .flex_1()
          .min_w_0()
          .text_sm()
          .truncate()
          .child(network.ssid.clone()),
      )
      .when(network.secured, |d| {
        d.child(Icon::new(IconName::Lock).small().mr_1())
      })
      .when(network.status == WifiStatus::Connected, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Success)
            .child("Connected"),
        )
      })
      .when(network.status == WifiStatus::Connecting, |d| {
        d.child(Spinner::new().small())
      })
      .when(network.status == WifiStatus::NeedAuth, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Warning)
            .child("Password"),
        )
      })
      .when(network.status == WifiStatus::Saved, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Secondary)
            .child("Saved"),
        )
      })
      .when_else(
        network.status != WifiStatus::New && network.status != WifiStatus::Saved,
        |d| {
          d.child(
            Button::new(format!("wifi-disconnect-{}", network.ssid))
              .icon(IconName::Unplug)
              .small()
              .tooltip("Disconnect")
              .cursor_pointer()
              .on_click(cx.async_listener(
                move |_, _, _, cx| cx.network_manager().disconnect(&interface, cx),
                |_, result, _| {
                  result.log_err().ok();
                },
              )),
          )
        },
        |d| {
          d.cursor_pointer().on_click(cx.async_listener(
            {
              let ssid = network.ssid.clone();
              move |_, _, _, cx| cx.network_manager().connect_wifi(ssid.clone(), cx)
            },
            |_, result, _| {
              result.log_err().ok();
            },
          ))
        },
      )
      .when(network.status != WifiStatus::New, |d| {
        d.child(
          Button::new(format!("wifi-forget-{}", network.ssid))
            .icon(IconName::Trash)
            .small()
            .tooltip("Forget")
            .cursor_pointer()
            .on_click(cx.async_listener(
              {
                let ssid = network.ssid.clone();
                move |_, _, _, cx| cx.network_manager().forget_wifi(ssid.clone(), cx)
              },
              |_, result, _| {
                result.log_err().ok();
              },
            )),
        )
      })
  }

  fn vpns(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let vpns = cx.network_manager().list_vpns(cx);

    if vpns.is_empty() {
      return Empty::new().into_any_element();
    }

    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(vpns.len() > 2, |d| d.min_h(px(128.)), |d| d.flex_shrink_0())
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("VPNs")),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .children(vpns.iter().map(|v| self.vpn(theme, v, cx))),
      )
      .into_any_element()
  }

  fn vpn(&self, theme: &Theme, vpn: &Vpn, cx: &Context<'_, Self>) -> impl IntoElement {
    let up = vpn.state == ActiveConnectionState::Activated
      || vpn.state == ActiveConnectionState::Activating;

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(if up {
          IconName::ShieldCheck
        } else {
          IconName::Shield
        })
        .small(),
      )
      .child(div().text_sm().truncate().child(vpn.name.clone()))
      .child(
        div()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(if vpn.kind == VpnKind::WireGuard {
            "WireGuard"
          } else {
            "VPN"
          }),
      )
      .when(vpn.state == ActiveConnectionState::Activated, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Success)
            .child("Connected"),
        )
      })
      .child(
        Button::new(format!("vpn-{}", vpn.name))
          .small()
          .cursor_pointer()
          .ml_auto()
          .loading(
            vpn.state == ActiveConnectionState::Activating
              || vpn.state == ActiveConnectionState::Deactivating,
          )
          .icon(if up { IconName::Unplug } else { IconName::Plug })
          .on_click(cx.async_listener(
            {
              let uuid = vpn.uuid.clone();
              move |_, _, _, cx| {
                let disconnect = cx.network_manager().disconnect_vpn(uuid.clone());
                let connect = cx.network_manager().connect_vpn(uuid.clone());

                async move { if up { disconnect.await } else { connect.await } }
              }
            },
            |_, result, _| {
              result.log_err().ok();
            },
          )),
      )
  }

  fn interfaces(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let interfaces = cx.network_manager().list_interfaces(cx);

    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        interfaces.len() > 2,
        |d| d.min_h(px(128.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("Interfaces")),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .children(interfaces.iter().map(|i| self.interface(theme, i, cx))),
      )
      .into_any_element()
  }

  fn interface(
    &self,
    theme: &Theme,
    interface: &Interface,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let (status, loading) = match interface.state {
      DeviceState::Activated => (address(interface), false),
      DeviceState::Prepare
      | DeviceState::Config
      | DeviceState::NeedAuth
      | DeviceState::IpConfig
      | DeviceState::IpCheck
      | DeviceState::Secondaries => ("Connecting".into(), true),
      DeviceState::Deactivating => ("Disconnecting".into(), true),
      DeviceState::Disconnected
      | DeviceState::Unmanaged
      | DeviceState::Unavailable
      | DeviceState::Failed
      | DeviceState::Unknown => ("Disconnected".into(), false),
    };

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(match interface.kind {
          InterfaceType::Wired => IconName::EthernetPort,
          InterfaceType::Wireless => IconName::Wifi,
        })
        .small(),
      )
      .child(div().text_sm().truncate().child(interface.name.clone()))
      .child(
        div()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(status),
      )
      .child(
        Button::new(format!("interface-{}", interface.name))
          .small()
          .loading(loading)
          .icon(if interface.state == DeviceState::Activated {
            IconName::Unplug
          } else {
            IconName::Plug
          })
          .cursor_pointer()
          .ml_auto()
          .on_click(cx.async_listener(
            {
              let name = interface.name.clone();
              let state = interface.state;
              move |_, _, _, cx| {
                let disconnect = cx.network_manager().disconnect(&name, cx);
                let connect = cx.network_manager().connect(&name, cx);

                async move {
                  if state == DeviceState::Activated {
                    disconnect.await
                  } else {
                    connect.await
                  }
                }
              }
            },
            |_, result, _| {
              result.log_err().ok();
            },
          )),
      )
  }
}

impl Render for NetworkPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.status(theme, cx))
      .child(self.wifi(theme, cx))
      .child(self.vpns(theme, cx))
      .child(self.interfaces(theme, cx))
  }
}
