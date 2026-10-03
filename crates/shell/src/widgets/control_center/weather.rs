use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{Standalone, WeatherPanel};

pub struct WeatherButton;

impl Widget for WeatherButton {
  const NAME: &'static str = "weather";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    WeatherButton
  }
}

impl Render for WeatherButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<WeatherPanel>>::new(cx, "weather-button", IconName::CloudSun)
  }
}
