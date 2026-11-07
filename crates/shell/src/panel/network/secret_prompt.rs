use corona_network_manager::{NetworkManagerExt, Secret, SecretKind};
use corona_components::components::{input::on_enter, modal::Dialog};
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window,
  base::input::InputState,
  component::{Sizable, Theme, input::Input},
  div,
  prelude::FluentBuilder,
};

use crate::panel::network::NetworkPanel;
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

pub struct SecretPrompt {
  password: Entity<InputState>,
  identity: Option<Entity<InputState>>,
  /// the request it was built for, a newer one replacing it gets a fresh prompt
  connection: OwnedObjectPath,
  setting: String,
  _submit: Vec<Subscription>,
}

impl NetworkPanel {
  pub fn sync_secret_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(request) = cx.network_manager().secret_request(cx) else {
      self.secret_prompt = None;
      return;
    };
    if self
      .secret_prompt
      .as_ref()
      .is_some_and(|p| p.connection == request.connection && p.setting == request.setting)
    {
      return;
    }
    let (connection, setting) = (request.connection.clone(), request.setting.clone());
    let identity = (request.kind == SecretKind::Enterprise && request.identity.is_none())
      .then(|| cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.network.username"))));
    let password = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder(t!("app.network.password"))
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
      connection,
      setting,
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

    Some(
      Dialog::new(
        "secret",
        t!("app.network.password_for", name = request.name),
        cx.listener(|_, _, _, cx| cx.network_manager().clone().answer_secret(cx, None)),
      )
      .when(request.retry, |d| {
        d.child(
          div()
            .text_xs()
            .text_color(theme.colors.danger)
            .child(t!("app.network.wrong_password")),
        )
      })
      .when_some(prompt.identity.as_ref(), |d, identity| {
        d.child(Input::new(identity).small())
      })
      .child(Input::new(&prompt.password).mask_toggle().small())
      .confirm(
        t!("app.common.connect"),
        false,
        false,
        cx.listener(|this, _, window, cx| this.submit_secret(window, cx)),
      ),
    )
  }
}
