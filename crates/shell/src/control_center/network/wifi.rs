use corona_components::async_listener::AsyncListenerExt;
use corona_network_manager::{Interface, NetworkManagerExt, WifiNetwork, WifiStatus};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
    spinner::Spinner,
    switch::Switch,
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::network::{LoadingState, NetworkPanel};

impl NetworkPanel {
  pub fn wifi(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
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
      .border_color(theme.border)
      .border_1()
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("Wi-Fi"))
          .child(
            Button::new("join-hidden-network")
              .icon(IconName::Plus)
              .primary()
              .disabled(device.is_none())
              .small()
              .ml_auto()
              .cursor_pointer()
              .tooltip("Join hidden network")
              .on_click(cx.listener(|this, _, window, cx| this.open_hidden_prompt(window, cx))),
          )
          .child(
            Button::new("wifi-rescan")
              .icon(IconName::RefreshCw)
              .disabled(device.is_none())
              .small()
              .cursor_pointer()
              .tooltip("Scan for networks")
              .loading(self.wifi_scanning == LoadingState::Loading)
              .when_else(
                self.wifi_scanning == LoadingState::Error,
                |b| {
                  b.with_variant(ButtonVariant::Danger)
                    .icon(IconName::RotateCw)
                },
                |b| b.icon(IconName::RefreshCw),
              )
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.wifi_scanning = LoadingState::Loading;
                  cx.network_manager().rescan(cx)
                },
                |this, result, _| this.wifi_scanning = result.log_err().into(),
              )),
          )
          .child(
            Switch::new("wifi-enabled")
              .checked(enabled)
              .disabled(!supported)
              .on_change(cx.async_listener(
                |_, checked, _, cx| cx.network_manager().set_wifi_enabled(*checked),
                |this, result, _| {
                  if let Err(e) = result.log_err() {
                    this.error = Some(e.to_string());
                  }
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
              .with_variant(ButtonVariant::Danger)
              .tooltip("Disconnect")
              .cursor_pointer()
              .on_click(cx.async_listener(
                move |_, _, _, cx| cx.network_manager().disconnect(&interface, cx),
                |this, result, _| {
                  if let Err(e) = result.log_err() {
                    this.error = Some(e.to_string());
                  }
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
            |this, result, _| {
              if let Err(e) = result.log_err() {
                this.error = Some(e.to_string());
              }
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
                move |_, _, _, cx| {
                  cx.stop_propagation();
                  cx.network_manager().forget_wifi(ssid.clone(), cx)
                }
              },
              |this, result, _| {
                if let Err(e) = result.log_err() {
                  this.error = Some(e.to_string());
                }
              },
            )),
        )
      })
  }
}
