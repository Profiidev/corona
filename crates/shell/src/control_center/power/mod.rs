use anyhow::Result;
use corona_components::components::card::{CardExt, ErrorCard};
use corona_power::PowerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::{ActiveTheme, Theme},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};

mod battery;
mod devices;
mod profiles;

pub(crate) use battery::icon as battery_icon;

pub struct PowerPanel {
  error: Option<String>,
  _subscriptions: [Subscription; 4],
}

impl ControlCenterPanel for PowerPanel {
  const TYPE: ControlCenterType = ControlCenterType::Power;

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let power = cx.power().clone();

    let subscriptions = [
      cx.observe(&power.status, |_, _, cx| cx.notify()),
      cx.observe(&power.battery, |_, _, cx| cx.notify()),
      cx.observe(&power.devices, |_, _, cx| cx.notify()),
      cx.observe(&power.profiles, |_, _, cx| cx.notify()),
    ];

    Self {
      error: None,
      _subscriptions: subscriptions,
    }
  }
}

fn card(theme: &Theme) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(theme)
}

impl PowerPanel {
  fn show_error(&mut self, result: Result<()>, _: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
  }

  fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    let error = self.error.clone()?;
    Some(
      ErrorCard::new("power-error-dismiss", error).on_dismiss(cx.listener(|this, _, _, cx| {
        this.error = None;
        cx.notify();
      })),
    )
  }
}

impl Render for PowerPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(self.error(cx), |d, error| d.child(error))
      .when_some(self.battery(theme, cx), |d, battery| d.child(battery))
      .when_some(self.profiles(theme, cx), |d, profiles| d.child(profiles))
      .when_some(self.battery_details(theme, cx), |d, details| {
        d.child(details)
      })
      .when_some(self.devices(theme, cx), |d, devices| d.child(devices))
  }
}
