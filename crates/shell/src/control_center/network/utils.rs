use corona_components::components::card::ErrorCard;
use corona_network_manager::{FailReason, Interface, NetworkManagerExt};
use gpui_kit::Context;

use crate::control_center::network::NetworkPanel;
use rust_i18n::t;

pub fn address(i: &Interface) -> String {
  if let Some(addr) = i.ip {
    format!("{}/{}", addr.address, addr.prefix)
  } else {
    t!("app.network.no_address").into()
  }
}

impl NetworkPanel {
  pub fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    if let Some(error) = &self.error {
      return Some(
        ErrorCard::new("error-dismiss", error.clone()).on_dismiss(cx.listener(|this, _, _, cx| {
          this.error = None;
          cx.notify();
        })),
      );
    }

    let failure = cx.network_manager().wifi_failure(cx)?;
    let error = match failure.reason {
      FailReason::SsidNotFound => t!("app.network.fail.not_found").to_string(),
      FailReason::NoSecrets => t!("app.network.fail.no_secrets").to_string(),
      FailReason::Other(code) => t!("app.network.fail.other", code = code).to_string(),
    };
    Some(ErrorCard::new("wifi-failure", error))
  }
}

#[cfg(test)]
mod tests {
  use corona_network_manager::{DeviceState, InterfaceType};

  use super::*;

  #[test]
  fn address_missing() {
    let interface = Interface {
      path: zbus::zvariant::OwnedObjectPath::try_from("/test").unwrap(),
      name: "eth0".into(),
      ip: None,
      kind: InterfaceType::Wired,
      state: DeviceState::Disconnected,
    };
    assert_eq!(address(&interface), "No address");
  }
}
