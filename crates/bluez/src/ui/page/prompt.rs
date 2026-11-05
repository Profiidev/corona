use crate::{BluetoothExt, PairingAnswer, PairingKind, parse_passkey};
use corona_components::components::{input::on_enter, modal::Dialog};
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window,
  base::{StyledExt, input::InputState},
  component::{Sizable, input::Input},
  div,
  prelude::FluentBuilder,
};

use crate::ui::page::BluetoothPanel;
use rust_i18n::t;

pub struct PairingPrompt {
  /// The request it was made for: another one, even a retry, starts with an empty code
  id: u64,
  code: Entity<InputState>,
  _submit: Subscription,
}

fn needs_code(kind: &PairingKind) -> bool {
  matches!(kind, PairingKind::PinCode | PairingKind::Passkey)
}

impl BluetoothPanel {
  pub fn sync_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(request) = cx.bluetooth().pairing_request(cx) else {
      self.prompt = None;
      return;
    };
    let id = request.id;
    if (self.prompt.as_ref()).is_some_and(|p| p.id == id) {
      return;
    }
    let code =
      cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.bluetooth.prompt.code")));
    code.update(cx, |input, cx| input.focus(window, cx));
    let submit = on_enter(&code, window, cx, |this, _, cx| this.answer(true, cx));
    self.prompt = Some(PairingPrompt {
      id,
      code,
      _submit: submit,
    });
  }

  fn answer(&mut self, accept: bool, cx: &mut Context<Self>) {
    let bluetooth = cx.bluetooth().clone();
    let Some(kind) = bluetooth.pairing_request(cx).map(|r| r.kind.clone()) else {
      return;
    };
    let answer = match (&self.prompt, accept) {
      (_, false) => PairingAnswer::Reject,
      (Some(prompt), true) if needs_code(&kind) => {
        let code = prompt.code.read(cx).value().to_string();
        // an invalid passkey keeps the dialog open instead of rejecting the pairing
        if kind == PairingKind::Passkey && parse_passkey(code.trim()).is_none() {
          return;
        }
        PairingAnswer::Code(code)
      }
      _ => PairingAnswer::Accept,
    };
    bluetooth.answer_pairing(cx, answer);
  }

  pub fn pairing_prompt(&self, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.prompt.as_ref()?;
    let bluetooth = cx.bluetooth();
    let request = bluetooth.pairing_request(cx)?;
    let name = bluetooth
      .list_devices(cx)
      .iter()
      .find(|d| d.path.as_str() == request.device)
      .map_or_else(|| request.device.clone(), |d| d.name.clone());

    // a passkey is always 6 digits, zero padded; a PIN is shown as it is
    let (hint, shown) = match &request.kind {
      PairingKind::Confirm { passkey } => (
        t!("app.bluetooth.prompt.confirm_hint"),
        Some(format!("{passkey:06}")),
      ),
      PairingKind::Authorize => (t!("app.bluetooth.prompt.authorize_hint"), None),
      PairingKind::PinCode => (t!("app.bluetooth.prompt.pin_hint"), None),
      PairingKind::Passkey => (t!("app.bluetooth.prompt.passkey_hint"), None),
      PairingKind::DisplayPasskey { passkey } => (
        t!("app.bluetooth.prompt.display_hint"),
        Some(format!("{passkey:06}")),
      ),
      PairingKind::DisplayPinCode { pin } => {
        (t!("app.bluetooth.prompt.display_hint"), Some(pin.clone()))
      }
    };
    let accept_label = match request.kind {
      PairingKind::Confirm { .. } => Some(t!("app.common.confirm")),
      PairingKind::DisplayPasskey { .. } | PairingKind::DisplayPinCode { .. } => None,
      _ => Some(t!("app.bluetooth.pair")),
    };

    Some(
      Dialog::new(
        "bt-pair",
        t!("app.bluetooth.prompt.title", name = name),
        cx.listener(|this, _, _, cx| this.answer(false, cx)),
      )
      .message(hint)
      .when_some(shown, |d, shown| {
        d.child(div().flex().justify_center().p_2().font_bold().child(shown))
      })
      .when(needs_code(&request.kind), |d| {
        d.child(Input::new(&prompt.code).small())
      })
      .when_some(accept_label, |d, label| {
        d.confirm(
          label,
          false,
          false,
          cx.listener(|this, _, _, cx| this.answer(true, cx)),
        )
      }),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn needs_code() {
    assert!(super::needs_code(&PairingKind::PinCode));
    assert!(super::needs_code(&PairingKind::Passkey));
    assert!(!super::needs_code(&PairingKind::Authorize));
    assert!(!super::needs_code(&PairingKind::Confirm { passkey: 1 }));
    assert!(!super::needs_code(&PairingKind::DisplayPasskey {
      passkey: 1
    }));
    assert!(!super::needs_code(&PairingKind::DisplayPinCode {
      pin: "1234".into()
    }));
  }
}
