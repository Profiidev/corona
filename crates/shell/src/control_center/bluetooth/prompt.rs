use corona_bluez::{BluetoothExt, PairingAnswer, PairingKind};
use corona_components::components::{input::on_enter, modal::modal};
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

use crate::control_center::bluetooth::BluetoothPanel;

pub struct PairingPrompt {
  code: Entity<InputState>,
  _submit: Subscription,
}

fn needs_code(kind: PairingKind) -> bool {
  matches!(kind, PairingKind::PinCode | PairingKind::Passkey)
}

impl BluetoothPanel {
  pub fn sync_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if cx.bluetooth().pairing_request(cx).is_none() {
      self.prompt = None;
      return;
    }
    if self.prompt.is_some() {
      return;
    }
    let code = cx.new(|cx| InputState::new(window, cx).placeholder("Code"));
    code.update(cx, |input, cx| input.focus(window, cx));
    let submit = on_enter(&code, window, cx, |this, _, cx| this.answer(true, cx));
    self.prompt = Some(PairingPrompt {
      code,
      _submit: submit,
    });
  }

  fn answer(&mut self, accept: bool, cx: &mut Context<Self>) {
    let bluetooth = cx.bluetooth().clone();
    let Some(kind) = bluetooth.pairing_request(cx).map(|r| r.kind) else {
      return;
    };
    let answer = match (&self.prompt, accept) {
      (_, false) => PairingAnswer::Reject,
      (Some(prompt), true) if needs_code(kind) => {
        PairingAnswer::Code(prompt.code.read(cx).value().to_string())
      }
      _ => PairingAnswer::Accept,
    };
    bluetooth.answer_pairing(cx, answer);
  }

  pub fn pairing_prompt(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.prompt.as_ref()?;
    let bluetooth = cx.bluetooth();
    let request = bluetooth.pairing_request(cx)?;
    let name = bluetooth
      .list_devices(cx)
      .iter()
      .find(|d| d.path.as_str() == request.device)
      .map_or_else(|| request.device.clone(), |d| d.name.clone());

    let (hint, passkey) = match request.kind {
      PairingKind::Confirm { passkey } => (
        "Confirm the code matches the one on the device",
        Some(passkey),
      ),
      PairingKind::Authorize => ("Allow this device to pair?", None),
      PairingKind::PinCode => ("Enter the PIN for the device", None),
      PairingKind::Passkey => ("Enter the passkey shown on the device", None),
      PairingKind::DisplayPasskey { passkey } => ("Type this code on the device", Some(passkey)),
    };
    let accept_label = match request.kind {
      PairingKind::Confirm { .. } => Some("Confirm"),
      PairingKind::DisplayPasskey { .. } => None,
      _ => Some("Pair"),
    };

    Some(modal(
      theme,
      div()
        .child(
          div()
            .font_bold()
            .text_sm()
            .truncate()
            .child(format!("Pair {name}")),
        )
        .child(
          div()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(hint),
        )
        .when_some(passkey, |d, passkey| {
          // always 6 digits, zero padded
          d.child(
            div()
              .flex()
              .justify_center()
              .p_2()
              .font_bold()
              .child(format!("{passkey:06}")),
          )
        })
        .when(needs_code(request.kind), |d| {
          d.child(Input::new(&prompt.code).small())
        })
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new("bt-pair-cancel")
                .label("Cancel")
                .small()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.answer(false, cx))),
            )
            .when_some(accept_label, |d, label| {
              d.child(
                Button::new("bt-pair-accept")
                  .primary()
                  .label(label)
                  .small()
                  .cursor_pointer()
                  .on_click(cx.listener(|this, _, _, cx| this.answer(true, cx))),
              )
            }),
        ),
      cx.listener(|this, _, _, cx| this.answer(false, cx)),
    ))
  }
}
