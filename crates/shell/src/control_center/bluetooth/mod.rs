use std::time::Duration;

use anyhow::Result;
use corona_bluez::BluetoothExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div, prelude::FluentBuilder,
};

use crate::control_center::{
  ControlCenterPanel, bluetooth::prompt::PairingPrompt, variants::ControlCenterType,
};

mod devices;
mod prompt;
mod status;

const SCAN: Duration = Duration::from_secs(30);

pub struct BluetoothPanel {
  error: Option<String>,
  prompt: Option<PairingPrompt>,
  _subscriptions: [Subscription; 3],
}

impl ControlCenterPanel for BluetoothPanel {
  const TYPE: ControlCenterType = ControlCenterType::Bluetooth;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let bluetooth = cx.bluetooth().clone();
    let subscriptions = [
      cx.observe(&bluetooth.adapter, |_, _, cx| cx.notify()),
      cx.observe(&bluetooth.devices, |_, _, cx| cx.notify()),
      cx.observe_in(&bluetooth.pairing_request, window, |this, _, window, cx| {
        this.sync_prompt(window, cx);
        cx.notify();
      }),
    ];
    cx.defer_in(window, |this, window, cx| this.sync_prompt(window, cx));

    Self {
      error: None,
      prompt: None,
      _subscriptions: subscriptions,
    }
  }
}

impl BluetoothPanel {
  fn show_error(&mut self, result: Result<()>, _: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
  }

  fn start_scan(cx: &mut Context<Self>) -> impl Future<Output = Result<()>> + use<> {
    let start = cx.bluetooth().start_discovery(cx);
    cx.spawn(async move |_, cx| {
      cx.background_executor().timer(SCAN).await;
      let stop = cx.update(|cx| {
        let bluetooth = cx.bluetooth();
        bluetooth
          .adapter(cx)
          .is_some_and(|a| a.discovering)
          .then(|| bluetooth.stop_discovery(cx))
      });
      if let Some(stop) = stop {
        stop.await.log_err().ok();
      }
    })
    .detach();
    start
  }
}

impl Render for BluetoothPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let powered = cx.bluetooth().adapter(cx).is_some_and(|a| a.powered);

    div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.status(theme, cx))
      .when_some(self.error(theme, cx), |d, error| d.child(error))
      .when(powered, |d| {
        d.child(self.paired(theme, cx))
          .when_some(self.available(theme, cx), |d, available| d.child(available))
      })
      .when_some(self.pairing_prompt(theme, cx), |d, prompt| d.child(prompt))
  }
}
