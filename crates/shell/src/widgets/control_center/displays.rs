use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{DisplaysPanel, Standalone};

pub struct DisplaysButton;

impl Widget for DisplaysButton {
  const NAME: &'static str = "displays";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    DisplaysButton
  }
}

impl Render for DisplaysButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<DisplaysPanel>>::new(cx, "displays-button", IconName::Monitor)
  }
}
