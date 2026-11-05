use corona_components::assets::icons::IconName;
use gpui_kit::{Context, IntoElement, Render, Window};
use uuid::Uuid;

use crate::{
  bar::{Button, Widget},
  control_center::ControlCenter,
};

/// Opens the control center
pub struct ControlCenterButton;

impl Widget for ControlCenterButton {
  const NAME: &'static str = "control_center";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    ControlCenterButton
  }
}

impl Render for ControlCenterButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, ControlCenter>::new(cx, "control-panel-button", IconName::Nixos)
  }
}
