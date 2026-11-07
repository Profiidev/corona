use corona_components::components::card::PageError;
use std::{sync::atomic::Ordering, time::Duration};

use corona_bluez::BluetoothExt;
use anyhow::Result;
use corona_surface::{
  control_center::{ControlCenterExt, ControlCenterPage},
  panel::PanelState,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div, prelude::FluentBuilder,
};

use prompt::PairingPrompt;

mod devices;
mod prompt;
mod status;

pub(super) fn register(cx: &mut App) {
  cx.control_center_mut().register::<BluetoothPanel>();
  open_for_pairing(cx);
}

/// Shows the page for a pairing request that comes in while it is closed: nothing else
/// renders the prompt, and the call hangs until BlueZ gives up
fn open_for_pairing(cx: &mut App) {
  let request = cx.bluetooth().pairing_request.clone();
  cx.observe(&request, |request, cx| {
    if request.read(cx).is_some() && !PanelState::is_open(BluetoothPanel::ID, cx) {
      PanelState::show(BluetoothPanel::ID, false, cx).log_err().ok();
    }
  })
  .detach();
}

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
    let bluetooth = cx.bluetooth();
    let start = bluetooth.start_discovery(cx);
    let scan = bluetooth.scan.clone();
    let id = scan.load(Ordering::Relaxed);
    cx.spawn(async move |_, cx| {
      cx.background_executor().timer(SCAN).await;
      let stop = cx.update(|cx| {
        let bluetooth = cx.bluetooth();
        // a later scan has its own timer
        (scan.load(Ordering::Relaxed) == id)
          .then_some(())
          .and_then(|()| bluetooth.adapter(cx))
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
      .when_some(self.pairing_prompt(cx), |d, prompt| d.child(prompt))
  }
}
