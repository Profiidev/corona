use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{PowerPanel, Standalone};

pub struct PowerButton;

impl Widget for PowerButton {
  const NAME: &'static str = "power";

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    PowerButton
  }
}

impl Render for PowerButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<PowerPanel>>::new(cx, "power-button", IconName::Zap)
  }
}
