use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_network_manager::{NetworkManagerExt, NmConnectivityState};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::network::{LoadingState, NetworkPanel, utils::address};
use crate::icons::interface_icon;

impl NetworkPanel {
  pub fn status(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let primary = cx.network_manager().primary_interface(cx);
    let state = cx.network_manager().connectivity(cx);
    let connectivity_check_enabled = cx.network_manager().connectivity_check(cx).is_some();

    let icon = interface_icon(primary);

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
      .card(theme)
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
}
