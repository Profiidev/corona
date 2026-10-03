use corona_bluez::BluetoothExt;
use corona_surface::bar::Widget;
use gpui_kit::{Context, IntoElement, Render, Subscription, Window, assets::IconName};
use uuid::Uuid;

use crate::{
  control_center::{BluetoothPanel, Standalone},
  widgets::button::Button,
};

pub struct BluetoothButton {
  _subscription: Subscription,
}

impl Widget for BluetoothButton {
  const NAME: &'static str = "bluetooth";

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let adapter = cx.bluetooth().adapter.clone();
    Self {
      _subscription: cx.observe(&adapter, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for BluetoothButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let (icon, unavailable) = match cx.bluetooth().adapter(cx) {
      None => (IconName::BluetoothOff, true),
      Some(a) if a.powered => (IconName::Bluetooth, false),
      Some(_) => (IconName::BluetoothOff, false),
    };

    Button::<_, Standalone<BluetoothPanel>>::new(cx, "bluetooth-button", icon).danger(unavailable)
  }
}
