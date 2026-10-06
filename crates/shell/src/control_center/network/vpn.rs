use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_network_manager::{ActiveConnectionState, NetworkManagerExt, Vpn, VpnKind};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::network::NetworkPanel;

impl NetworkPanel {
  pub fn vpns(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let vpns = cx.network_manager().list_vpns(cx);

    if vpns.is_empty() {
      return None;
    }

    Some(
      div()
        .flex()
        .flex_col()
        .w_full()
        .when_else(vpns.len() > 2, |d| d.min_h(px(128.)), |d| d.flex_shrink_0())
        .gap_2()
        .p_2()
        .card(cx)
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
        ),
    )
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
          .with_variant(if up {
            ButtonVariant::Danger
          } else {
            ButtonVariant::Primary
          })
          .on_click(cx.async_listener(
            {
              let uuid = vpn.uuid.clone();
              move |_, _, _, cx| {
                let disconnect = cx.network_manager().disconnect_vpn(uuid.clone());
                let connect = cx.network_manager().connect_vpn(uuid.clone());

                async move { if up { disconnect.await } else { connect.await } }
              }
            },
            |this, result, _| {
              if let Err(e) = result.log_err() {
                this.error = Some(e.to_string());
              }
            },
          )),
      )
  }
}
