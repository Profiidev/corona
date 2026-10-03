use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{BrightnessPanel, Standalone};

pub struct BrightnessButton;

impl Widget for BrightnessButton {
  const NAME: &'static str = "brightness";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    BrightnessButton
  }
}

impl Render for BrightnessButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<BrightnessPanel>>::new(cx, "brightness-button", IconName::Sun)
  }
}
