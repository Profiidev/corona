use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{MediaPanel, Standalone};

pub struct MediaButton;

impl Widget for MediaButton {
  const NAME: &'static str = "media";

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    MediaButton
  }
}

impl Render for MediaButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<MediaPanel>>::new(cx, "media-button", IconName::Music)
  }
}
