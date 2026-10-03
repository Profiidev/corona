use corona_components::components::card::ErrorCard;
use corona_network_manager::{FailReason, Interface, NetworkManagerExt};
use gpui_kit::Context;

use crate::control_center::network::NetworkPanel;

pub fn address(i: &Interface) -> String {
  if let Some(addr) = i.ip {
    format!("{}/{}", addr.address, addr.prefix)
  } else {
    "No address".to_string()
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
      FailReason::SsidNotFound => "Network not found".to_string(),
      FailReason::NoSecrets => "No password provided".to_string(),
      FailReason::Other(code) => format!("Connection failed: {}", code),
    };
    Some(ErrorCard::new("wifi-failure", error))
  }
}
