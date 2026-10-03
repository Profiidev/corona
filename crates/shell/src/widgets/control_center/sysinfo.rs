use corona_surface::bar::Widget;
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::{
  control_center::{Standalone, SysinfoPanel},
  widgets::button::Button,
};

pub struct SysinfoButton;

impl Widget for SysinfoButton {
  const NAME: &'static str = "sysinfo";

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    SysinfoButton
  }
}

impl Render for SysinfoButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<SysinfoPanel>>::new(cx, "sysinfo-button", IconName::Activity)
  }
}
