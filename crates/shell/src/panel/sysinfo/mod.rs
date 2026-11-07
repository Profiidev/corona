use corona_sysinfo::SystemMonitorExt;
use corona_components::components::list::card;
use corona_surface::control_center::ControlCenterPage;
use gpui_kit::{
  Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::ActiveTheme, div,
};

pub(crate) mod details;
mod graphs;

pub struct SysinfoPanel {
  _subscriptions: [Subscription; 2],
}

impl ControlCenterPage for SysinfoPanel {
  const ID: &'static str = "sysinfo";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Activity
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.sysinfo").into()
  }

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let monitor = cx.system_monitor().clone();

    let subscriptions = [
      cx.observe(&monitor.info, |_, _, cx| cx.notify()),
      cx.observe(&monitor.sample, |_, _, cx| cx.notify()),
    ];

    Self {
      _subscriptions: subscriptions,
    }
  }
}

fn row(left: impl IntoElement, right: impl IntoElement) -> Div {
  div()
    .flex()
    .gap_2()
    .w_full()
    .child(div().flex_1().min_w_0().flex().child(left))
    .child(div().flex_1().min_w_0().flex().child(right))
}

impl Render for SysinfoPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(
        row(self.cpu(theme, cx), self.memory(theme, cx))
          .flex_1()
          .min_h_0(),
      )
      .child(match self.gpu(theme, cx) {
        Some(gpu) => row(gpu, self.network(theme, cx)).flex_1().min_h_0(),
        None => div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(self.network(theme, cx)),
      })
      .child(row(self.system(theme, cx), self.resources(theme, cx)).flex_none())
  }
}
