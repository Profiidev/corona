use corona_bluez::BluetoothExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{BluetoothPanel, Standalone};

pub struct BluetoothButton {
  _subscription: Subscription,
}

impl Widget for BluetoothButton {
  const NAME: &'static str = "bluetooth";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let adapter = cx.bluetooth().adapter.clone();
    Self {
      _subscription: cx.observe(&adapter, |_, _, cx| cx.notify()),
    }
  }
}

/// The icon and whether it shows as an error; `powered` is None without an adapter
fn icon(powered: Option<bool>) -> (IconName, bool) {
  match powered {
    None => (IconName::BluetoothOff, true),
    Some(true) => (IconName::Bluetooth, false),
    Some(false) => (IconName::BluetoothOff, false),
  }
}

impl Render for BluetoothButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let (icon, unavailable) = icon(cx.bluetooth().adapter(cx).map(|a| a.powered));

    Button::<_, Standalone<BluetoothPanel>>::new(cx, "bluetooth-button", icon).danger(unavailable)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn icon() {
    assert_eq!(super::icon(None), (IconName::BluetoothOff, true));
    assert_eq!(super::icon(Some(true)), (IconName::Bluetooth, false));
    assert_eq!(super::icon(Some(false)), (IconName::BluetoothOff, false));
  }
}
