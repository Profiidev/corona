use corona_network_manager::{FailReason, Interface, NetworkManagerExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{Sizable, Theme, button::Button},
  div,
};

use crate::control_center::network::NetworkPanel;

pub fn address(i: &Interface) -> String {
  if let Some(addr) = i.ip {
    format!("{}/{}", addr.address, addr.prefix)
  } else {
    "No address".to_string()
  }
}

impl NetworkPanel {
  pub fn error(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    if let Some(error) = &self.error {
      return Some(
        div()
          .flex()
          .gap_2()
          .p_2()
          .rounded_xl()
          .bg(theme.colors.accent)
          .border_color(theme.border)
          .border_1()
          .child(
            div()
              .text_sm()
              .text_color(theme.colors.danger)
              .truncate()
              .child(error.clone()),
          )
          .child(
            Button::new("error-dismiss")
              .small()
              .ml_auto()
              .icon(IconName::X)
              .cursor_pointer()
              .on_click(cx.listener(|this, _, _, cx| {
                this.error = None;
                cx.notify();
              })),
          ),
      );
    }

    let failure = cx.network_manager().wifi_failure(cx)?;

    let error = match failure.reason {
      FailReason::SsidNotFound => "Network not found".to_string(),
      FailReason::NoSecrets => "No password provided".to_string(),
      FailReason::Other(code) => format!("Connection failed: {}", code),
    };

    Some(
      div()
        .flex()
        .gap_2()
        .p_2()
        .rounded_xl()
        .bg(theme.colors.accent)
        .border_color(theme.border)
        .border_1()
        .child(
          div()
            .text_sm()
            .text_color(theme.colors.danger)
            .truncate()
            .child(error),
        ),
    )
  }
}
