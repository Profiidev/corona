use gpui_kit::{Context, IntoElement, Render, Window};

use crate::{
  assets::icons::IconName,
  bar::widgets::{Widget, button::Button},
  panel::ControlPanel,
};

pub struct ControlPanelButton;

impl Widget for ControlPanelButton {
  fn init(_cx: &mut Context<'_, Self>) -> Self {
    ControlPanelButton
  }
}

impl Render for ControlPanelButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::new(cx, "control-panel-button", IconName::Nixos, || ControlPanel)
  }
}
