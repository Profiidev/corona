use crate::NetworkManagerExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window};
use uuid::Uuid;

use crate::ui::{icons::primary_icon, network::NetworkPanel};
use corona_surface::control_center::Standalone;

pub struct NetworkButton {
  _subscriptions: [Subscription; 2],
}

impl Widget for NetworkButton {
  const NAME: &'static str = "network";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let primary = cx.network_manager().primary_interface.clone();
    let networks = cx.network_manager().wifi_networks.clone();
    Self {
      _subscriptions: [
        cx.observe(&primary, |_, _, cx| cx.notify()),
        cx.observe(&networks, |_, _, cx| cx.notify()),
      ],
    }
  }
}

impl Render for NetworkButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let nm = cx.network_manager();
    let icon = primary_icon(nm.primary_interface(cx), nm.list_wifi_networks(cx));
    Button::<_, Standalone<NetworkPanel>>::new(cx, "network-button", icon)
  }
}
