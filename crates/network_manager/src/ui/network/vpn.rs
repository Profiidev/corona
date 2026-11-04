use crate::{ActiveConnectionState, NetworkManagerExt, Vpn, VpnKind};
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::list::ListCard;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
};

use crate::ui::network::NetworkPanel;
use rust_i18n::t;

impl NetworkPanel {
  pub fn vpns(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let vpns = cx.network_manager().list_vpns(cx);

    if vpns.is_empty() {
      return None;
    }

    Some(
      ListCard::new("network-vpns", t!("app.network.vpns"))
        .children(vpns.iter().map(|v| self.vpn(theme, v, cx))),
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
            .child(t!("app.network.connected")),
        )
      })
      .child(
        Button::new(format!("vpn-{}", vpn.uuid))
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
            |this, result, _| this.error.set(result),
          )),
      )
  }
}
