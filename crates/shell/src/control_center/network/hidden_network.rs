use corona_components::components::{input::on_enter, modal::modal};
use corona_network_manager::{HiddenSecurity, NetworkManagerExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window,
  base::{IndexPath, StyledExt, input::InputState},
  component::{
    Sizable, Theme,
    button::{Button, ButtonVariants},
    input::Input,
    select::{Select, SelectEvent, SelectState},
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::network::NetworkPanel;
use rust_i18n::t;

pub struct HiddenPrompt {
  ssid: Entity<InputState>,
  password: Entity<InputState>,
  security_select: Entity<SelectState<Vec<String>>>,
  security: HiddenSecurity,
  _security_changed: Subscription,
  _submit: Vec<Subscription>,
}

const SECURITY_OPTIONS: [HiddenSecurity; 3] = [
  HiddenSecurity::Open,
  HiddenSecurity::Wpa,
  HiddenSecurity::Wpa3,
];

fn security_label(security: HiddenSecurity) -> String {
  match security {
    HiddenSecurity::Open => t!("app.network.hidden.open").into(),
    HiddenSecurity::Wpa => "WPA/WPA2".into(),
    HiddenSecurity::Wpa3 => "WPA3".into(),
  }
}

impl NetworkPanel {
  pub fn open_hidden_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let ssid = cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.network.hidden.ssid")));
    let password = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder(t!("app.network.password"))
    });
    let labels = SECURITY_OPTIONS.map(security_label).to_vec();
    let security_select =
      cx.new(|cx| SelectState::new(labels, Some(IndexPath::default().row(1)), window, cx));
    let security_changed = cx.subscribe(
      &security_select,
      |this, _, event: &SelectEvent<Vec<String>>, cx| {
        let SelectEvent::Confirm(Some(label)) = event else {
          return;
        };
        if let (Some(prompt), Some(security)) = (
          &mut this.hidden_prompt,
          SECURITY_OPTIONS
            .iter()
            .find(|s| security_label(**s) == *label),
        ) {
          prompt.security = *security;
          cx.notify();
        }
      },
    );
    ssid.update(cx, |input, cx| input.focus(window, cx));
    let submit = vec![
      on_enter(&ssid, window, cx, Self::join_hidden),
      on_enter(&password, window, cx, Self::join_hidden),
    ];
    self.hidden_prompt = Some(HiddenPrompt {
      ssid,
      password,
      security_select,
      security: HiddenSecurity::Wpa,
      _security_changed: security_changed,
      _submit: submit,
    });
    cx.notify();
  }

  fn join_hidden(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(prompt) = &self.hidden_prompt else {
      return;
    };
    let ssid = prompt.ssid.read(cx).value().trim().to_string();
    if ssid.is_empty() {
      prompt.ssid.update(cx, |input, cx| input.focus(window, cx));
      return;
    }
    let password = Some(prompt.password.read(cx).value().to_string())
      .filter(|password| prompt.security != HiddenSecurity::Open && !password.is_empty());
    let join = cx
      .network_manager()
      .join_hidden_wifi(ssid, prompt.security, password, cx);
    self.hidden_prompt = None;
    cx.spawn(async move |this, cx| {
      if let Err(e) = join.await.log_err() {
        this
          .update(cx, |this, cx| {
            this.error = Some(e.to_string());
            cx.notify();
          })
          .ok();
      }
    })
    .detach();
    cx.notify();
  }

  pub fn hidden_prompt(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.hidden_prompt.as_ref()?;

    Some(modal(
      theme,
      div()
        .child(
          div()
            .font_bold()
            .text_sm()
            .child(t!("app.network.hidden.title")),
        )
        .child(Input::new(&prompt.ssid).small())
        .child(Select::new(&prompt.security_select).small())
        .when(prompt.security != HiddenSecurity::Open, |d| {
          d.child(Input::new(&prompt.password).mask_toggle().small())
        })
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new("hidden-cancel")
                .label(t!("app.common.cancel"))
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                  this.hidden_prompt = None;
                  cx.notify();
                })),
            )
            .child(
              Button::new("hidden-connect")
                .primary()
                .label(t!("app.common.connect"))
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.join_hidden(window, cx))),
            ),
        ),
      cx.listener(|this, _, _, cx| {
        this.hidden_prompt = None;
        cx.notify();
      }),
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn security_labels_round_trip() {
    // the select hands back a label, which must map to one option
    for security in SECURITY_OPTIONS {
      let label = security_label(security);
      let found: Vec<_> = SECURITY_OPTIONS
        .iter()
        .filter(|s| security_label(**s) == label)
        .collect();
      assert_eq!(found, [&security]);
    }
  }
}
