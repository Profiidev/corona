use corona_network_manager::{NetworkManagerExt, Secret, SecretKind};
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window,
  base::{StyledExt, input::InputState},
  component::{
    Sizable, Theme,
    button::{Button, ButtonVariants},
    input::Input,
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::network::{
  NetworkPanel,
  utils::{on_enter, overlay},
};

pub struct SecretPrompt {
  password: Entity<InputState>,
  identity: Option<Entity<InputState>>,
  _submit: Vec<Subscription>,
}

impl NetworkPanel {
  pub fn sync_secret_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(request) = cx.network_manager().secret_request(cx) else {
      self.secret_prompt = None;
      return;
    };
    if self.secret_prompt.is_some() {
      return;
    }
    let identity = (request.kind == SecretKind::Enterprise && request.identity.is_none())
      .then(|| cx.new(|cx| InputState::new(window, cx).placeholder("Username")));
    let password = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder("Password")
    });
    password.update(cx, |input, cx| input.focus(window, cx));
    let submit = identity
      .iter()
      .chain([&password])
      .map(|input| on_enter(input, window, cx, Self::submit_secret))
      .collect();
    self.secret_prompt = Some(SecretPrompt {
      password,
      identity,
      _submit: submit,
    });
  }

  fn submit_secret(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
    let Some(prompt) = &self.secret_prompt else {
      return;
    };
    let secret = Secret {
      password: prompt.password.read(cx).value().to_string(),
      identity: prompt
        .identity
        .as_ref()
        .map(|i| i.read(cx).value().to_string()),
    };
    cx.network_manager().clone().answer_secret(cx, Some(secret));
  }

  pub fn secret_prompt(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.secret_prompt.as_ref()?;
    let request = cx.network_manager().secret_request(cx)?;

    Some(overlay(
      theme,
      div()
        .child(
          div()
            .font_bold()
            .text_sm()
            .truncate()
            .child(format!("Password for {}", request.name)),
        )
        .when(request.retry, |d| {
          d.child(
            div()
              .text_xs()
              .text_color(theme.colors.danger)
              .child("Wrong password, try again"),
          )
        })
        .when_some(prompt.identity.as_ref(), |d, identity| {
          d.child(Input::new(identity).small())
        })
        .child(Input::new(&prompt.password).mask_toggle().small())
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new("secret-cancel")
                .label("Cancel")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|_, _, _, cx| {
                  cx.network_manager().clone().answer_secret(cx, None);
                })),
            )
            .child(
              Button::new("secret-connect")
                .primary()
                .label("Connect")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.submit_secret(window, cx))),
            ),
        ),
      cx.listener(|_, _, _, cx| cx.network_manager().clone().answer_secret(cx, None)),
    ))
  }
}
