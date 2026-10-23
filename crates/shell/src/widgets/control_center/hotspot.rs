use corona_network_manager::NetworkManagerExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Task, Window,
  assets::IconName, div, prelude::FluentBuilder,
};
use uuid::Uuid;

use crate::control_center::{HotspotPanel, Standalone, poll_clients};

pub struct HotspotButton {
  clients: usize,
  _poll_clients: Option<Task<()>>,
  _subscription: Subscription,
}

impl Widget for HotspotButton {
  const NAME: &'static str = "hotspot";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let hotspot = cx.network_manager().hotspot.clone();
    let mut this = Self {
      clients: 0,
      _poll_clients: None,
      _subscription: cx.observe(&hotspot, |this, _, cx| this.poll(cx)),
    };
    this.poll(cx);
    this
  }
}

impl HotspotButton {
  fn poll(&mut self, cx: &mut Context<Self>) {
    self.clients = 0;
    self._poll_clients = poll_clients(cx, |this, clients| this.clients = clients.len());
    cx.notify();
  }
}

impl Render for HotspotButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let running = cx.network_manager().hotspot(cx).is_some();

    Button::<_, Standalone<HotspotPanel>>::new(cx, "hotspot-button", IconName::RadioTower)
      .dot(running)
      .when(running && self.clients > 0, |b| {
        b.suffix(div().text_xs().child(self.clients.to_string()))
      })
  }
}
