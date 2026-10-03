use corona_network_manager::NetworkManagerExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div, prelude::FluentBuilder,
};

use crate::control_center::{
  ControlCenterPanel,
  network::{hidden_network::HiddenPrompt, secret_prompt::SecretPrompt},
  variants::ControlCenterType,
};

mod hidden_network;
mod interface;
mod secret_prompt;
mod status;
mod utils;
mod vpn;
mod wifi;

#[derive(Debug, PartialEq, Eq)]
enum LoadingState {
  Idle,
  Loading,
  Error,
}

impl<T, E> From<Result<T, E>> for LoadingState {
  fn from(result: Result<T, E>) -> Self {
    if result.is_ok() {
      Self::Idle
    } else {
      Self::Error
    }
  }
}

pub struct NetworkPanel {
  connectivity_checking: LoadingState,
  wifi_scanning: LoadingState,
  error: Option<String>,
  secret_prompt: Option<SecretPrompt>,
  hidden_prompt: Option<HiddenPrompt>,
  _secret_request: Subscription,
}

impl ControlCenterPanel for NetworkPanel {
  const TYPE: ControlCenterType = ControlCenterType::Network;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let secret_request = cx.network_manager().secret_request.clone();
    let subscription = cx.observe_in(&secret_request, window, |this, _, window, cx| {
      this.sync_secret_prompt(window, cx)
    });
    cx.defer_in(window, |this, window, cx| {
      this.sync_secret_prompt(window, cx)
    });

    Self {
      connectivity_checking: LoadingState::Idle,
      wifi_scanning: LoadingState::Idle,
      error: None,
      secret_prompt: None,
      hidden_prompt: None,
      _secret_request: subscription,
    }
  }
}

impl Render for NetworkPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.status(theme, cx))
      .when_some(self.error(cx), |d, error| d.child(error))
      .child(self.wifi(theme, cx))
      .when_some(self.vpns(theme, cx), |d, vpns| d.child(vpns))
      .child(self.interfaces(theme, cx))
      .when_some(self.hidden_prompt(theme, cx), |d, prompt| d.child(prompt))
      .when_some(self.secret_prompt(theme, cx), |d, prompt| d.child(prompt))
  }
}
