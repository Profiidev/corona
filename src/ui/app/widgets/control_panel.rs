use gpui_kit::{Context, IntoElement, Render, Window};
use uuid::Uuid;

use crate::ui::{
  app::{control_center::ControlCenter, widgets::button::Button},
  assets::icons::IconName,
  bar::Widget,
};

pub struct ControlCenterButton;

impl Widget for ControlCenterButton {
  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    ControlCenterButton
  }
}

impl Render for ControlCenterButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, ControlCenter>::new(cx, "control-panel-button", IconName::Nixos)
  }
}
