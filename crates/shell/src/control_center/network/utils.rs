use corona_network_manager::{FailReason, Interface, NetworkManagerExt};
use gpui_kit::{
  App, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
  ParentElement, Styled, Subscription, Window,
  assets::IconName,
  base::input::{InputEvent, InputState},
  component::{Sizable, Theme, button::Button},
  div,
};

use crate::control_center::network::NetworkPanel;

pub fn on_enter(
  input: &Entity<InputState>,
  window: &mut Window,
  cx: &mut Context<NetworkPanel>,
  submit: fn(&mut NetworkPanel, &mut Window, &mut Context<NetworkPanel>),
) -> Subscription {
  cx.subscribe_in(
    input,
    window,
    move |this, _, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        submit(this, window, cx);
      }
    },
  )
}

pub fn overlay(
  theme: &Theme,
  card: Div,
  on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
  div()
    .on_mouse_down(MouseButton::Left, on_close)
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .rounded_xl()
    .bg(theme.colors.background.opacity(0.8))
    .occlude()
    .child(
      card
        .flex()
        .flex_col()
        .w_3_4()
        .gap_2()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.background)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
    )
}

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
