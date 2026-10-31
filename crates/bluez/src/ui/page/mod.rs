use corona_components::components::card::PageError;
use std::time::Duration;

use crate::BluetoothExt;
use anyhow::Result;
use corona_surface::control_center::ControlCenterPage;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div, prelude::FluentBuilder,
};

use crate::ui::page::prompt::PairingPrompt;

mod devices;
mod prompt;
mod status;

const SCAN: Duration = Duration::from_secs(30);

pub struct BluetoothPanel {
  error: PageError,
  prompt: Option<PairingPrompt>,
  _subscriptions: [Subscription; 3],
}

impl ControlCenterPage for BluetoothPanel {
  const ID: &'static str = "bluetooth";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Bluetooth
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.bluetooth").into()
  }

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
      error: PageError::default(),
      prompt: None,
      _subscriptions: subscriptions,
    }
  }
}

impl BluetoothPanel {
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
      .when_some(
        self
          .error
          .card("bt-error-dismiss", |this| &mut this.error, cx),
        |d, error| d.child(error),
      )
      .when(powered, |d| {
        d.child(self.paired(theme, cx))
          .when_some(self.available(theme, cx), |d, available| d.child(available))
      })
      .when_some(self.pairing_prompt(theme, cx), |d, prompt| d.child(prompt))
  }
}
