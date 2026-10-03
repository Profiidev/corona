use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{CalendarPanel, Standalone};

pub struct CalendarButton;

impl Widget for CalendarButton {
  const NAME: &'static str = "calendar";

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    CalendarButton
  }
}

impl Render for CalendarButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, Standalone<CalendarPanel>>::new(cx, "calendar-button", IconName::CalendarDays)
  }
}
