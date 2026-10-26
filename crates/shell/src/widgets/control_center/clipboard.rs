use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{ClipboardPanel, Standalone};

pub struct ClipboardButton;

impl Widget for ClipboardButton {
  const NAME: &'static str = "clipboard";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    ClipboardButton
  }
}

impl Render for ClipboardButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<ClipboardPanel>>::new(cx, "clipboard-button", IconName::ClipboardList)
  }
}
