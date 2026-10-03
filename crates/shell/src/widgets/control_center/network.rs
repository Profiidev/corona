use corona_network_manager::NetworkManagerExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window};
use uuid::Uuid;

use crate::{
  control_center::{NetworkPanel, Standalone},
  icons::interface_icon,
};

pub struct NetworkButton {
  _subscription: Subscription,
}

impl Widget for NetworkButton {
  const NAME: &'static str = "network";

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let primary = cx.network_manager().primary_interface.clone();
    Self {
      _subscription: cx.observe(&primary, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for NetworkButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let icon = interface_icon(cx.network_manager().primary_interface(cx));
    Button::<_, Standalone<NetworkPanel>>::new(cx, "network-button", icon)
  }
}
