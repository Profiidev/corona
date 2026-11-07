use corona_power::PowerExt;
use corona_components::components::card::PageError;
use corona_components::components::list::card;
use corona_surface::control_center::ControlCenterPage;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div, prelude::FluentBuilder,
};

pub(crate) mod battery;
pub(crate) mod icons;
mod devices;
mod profiles;

pub struct PowerPanel {
  error: PageError,
  _subscriptions: [Subscription; 3],
}

impl ControlCenterPage for PowerPanel {
  const ID: &'static str = "power";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Zap
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.power").into()
  }

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let power = cx.power().clone();

    let subscriptions = [
      cx.observe(&power.battery, |_, _, cx| cx.notify()),
      cx.observe(&power.devices, |_, _, cx| cx.notify()),
      cx.observe(&power.profiles, |_, _, cx| cx.notify()),
    ];

    Self {
      error: PageError::default(),
      _subscriptions: subscriptions,
    }
  }
}

impl PowerPanel {}

impl Render for PowerPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(
        self
          .error
          .card("power-error-dismiss", |this| &mut this.error, cx),
        |d, error| d.child(error),
      )
      .when_some(self.battery(theme, cx), |d, battery| d.child(battery))
      .when_some(self.profiles(theme, cx), |d, profiles| d.child(profiles))
      .when_some(self.battery_details(theme, cx), |d, details| {
        d.child(details)
      })
      .when_some(self.devices(theme, cx), |d, devices| d.child(devices))
  }
}
