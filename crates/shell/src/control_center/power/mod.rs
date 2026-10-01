use anyhow::Result;
use corona_power::PowerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  assets::IconName,
  component::{ActiveTheme, Sizable, Theme, button::Button},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::ControlCenterPanel;

mod battery;
mod devices;
mod profiles;

pub struct PowerPanel {
  error: Option<String>,
  _subscriptions: [Subscription; 4],
}

impl ControlCenterPanel for PowerPanel {
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
  div()
    .flex()
    .flex_col()
    .w_full()
    .gap_2()
    .p_2()
    .rounded_xl()
    .bg(theme.colors.accent)
    .border_color(theme.border)
    .border_1()
}

impl PowerPanel {
  fn show_error(&mut self, result: Result<()>, _: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
  }

  fn error(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let error = self.error.clone()?;
    Some(
      div()
        .flex()
        .gap_2()
        .p_2()
        .rounded_xl()
        .bg(theme.colors.accent)
        .border_color(theme.border)
        .border_1()
        .child(
          div()
            .text_sm()
            .text_color(theme.colors.danger)
            .truncate()
            .child(error),
        )
        .child(
          Button::new("power-error-dismiss")
            .small()
            .ml_auto()
            .icon(IconName::X)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
              this.error = None;
              cx.notify();
            })),
        ),
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
      .when_some(self.error(theme, cx), |d, error| d.child(error))
      .when_some(self.battery(theme, cx), |d, battery| d.child(battery))
      .when_some(self.profiles(theme, cx), |d, profiles| d.child(profiles))
      .when_some(self.battery_details(theme, cx), |d, details| {
        d.child(details)
      })
      .when_some(self.devices(theme, cx), |d, devices| d.child(devices))
  }
}
