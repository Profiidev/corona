use corona_network_manager::{Interface, InterfaceType, NetworkManagerExt, NmConnectivityState};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Window,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    ActiveTheme, Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
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

pub struct NetworkPanel {
  connectivity_checking: LoadingState,
}

impl ControlCenterPanel for NetworkPanel {
  fn init(_window: &mut Window, _cx: &mut Context<'_, Self>) -> Self {
    Self {
      connectivity_checking: LoadingState::Idle,
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
              .flex_col()
              .flex_1()
              .min_w_0()
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
              ),
          )
          .child(Tag::new().small().with_variant(variant).child(label))
          .child(
            Button::new("connectivity-check")
              .small()
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
              .on_click(cx.listener(|this, _, _, cx| {
                this.connectivity_checking = LoadingState::Loading;
                let check = cx.network_manager().check_connectivity();
                cx.spawn(async move |this, cx| {
                  let state = if check.await.log_err().is_ok() {
                    LoadingState::Idle
                  } else {
                    LoadingState::Error
                  };
                  this
                    .update(cx, |this, cx| {
                      this.connectivity_checking = state;
                      cx.notify();
                    })
                    .ok();
                })
                .detach();
                cx.notify();
              })),
          ),
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
  }
}
