use crate::{DeviceState, Interface, InterfaceType, NetworkManagerExt};
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::list::ListCard;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};

use crate::ui::network::{NetworkPanel, utils::address};
use rust_i18n::t;

impl NetworkPanel {
  pub fn interfaces(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let interfaces = cx.network_manager().list_interfaces(cx);

    ListCard::new("network-interfaces", t!("app.network.interfaces"))
      .children(interfaces.iter().map(|i| self.interface(theme, i, cx)))
  }

  fn interface(
    &self,
    theme: &Theme,
    interface: &Interface,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let status = match interface.state {
      DeviceState::Activated => address(interface),
      DeviceState::Unmanaged => t!("app.network.state.unmanaged").into(),
      DeviceState::Unavailable => t!("app.common.unavailable").into(),
      DeviceState::Disconnected => t!("app.network.state.disconnected").into(),
      DeviceState::Prepare => t!("app.network.state.preparing").into(),
      DeviceState::Config => t!("app.network.state.configuring").into(),
      DeviceState::NeedAuth => t!("app.network.state.needs_auth").into(),
      DeviceState::IpConfig => t!("app.network.state.getting_address").into(),
      DeviceState::IpCheck => t!("app.network.state.checking").into(),
      DeviceState::Secondaries => t!("app.network.state.dependencies").into(),
      DeviceState::Deactivating => t!("app.network.state.disconnecting").into(),
      DeviceState::Failed => t!("app.network.state.failed").into(),
      DeviceState::Unknown => t!("app.common.unknown").into(),
    };
    // Some(true): disconnect, also cancels an attempt still in progress, Some(false): connect,
    // None: NM can't act on the device right now
    let disconnect = match interface.state {
      DeviceState::Prepare
      | DeviceState::Config
      | DeviceState::NeedAuth
      | DeviceState::IpConfig
      | DeviceState::IpCheck
      | DeviceState::Secondaries
      | DeviceState::Activated => Some(true),
      DeviceState::Disconnected | DeviceState::Failed => Some(false),
      DeviceState::Unmanaged
      | DeviceState::Unavailable
      | DeviceState::Deactivating
      | DeviceState::Unknown => None,
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
      .when_some(disconnect, |d, disconnect| {
        d.child(
          Button::new(format!("interface-{}", interface.name))
            .small()
            .icon(if disconnect {
              IconName::Unplug
            } else {
              IconName::Plug
            })
            .with_variant(if disconnect {
              ButtonVariant::Danger
            } else {
              ButtonVariant::Primary
            })
            .tooltip(if disconnect {
              t!("app.common.disconnect")
            } else {
              t!("app.common.connect")
            })
            .cursor_pointer()
            .ml_auto()
            .on_click(cx.async_listener(
              {
                let name = interface.name.clone();
                move |_, _, _, cx| {
                  let disconnect_task = cx.network_manager().disconnect(&name, cx);
                  let connect_task = cx.network_manager().connect(&name, cx);

                  async move {
                    if disconnect {
                      disconnect_task.await
                    } else {
                      connect_task.await
                    }
                  }
                }
              },
              |this, result, _| this.error.set(result),
            )),
        )
      })
  }
}
