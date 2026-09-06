use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  Bounds, Context, IntoElement, Render, Styled, Window,
  base::ElementExt,
  component::{self, button::ButtonVariants},
};

use crate::{assets::icons::IconName, bar::toggle_panel, bar::widgets::Widget, panel::ControlPanel};

pub struct Button;

impl Widget for Button {}

impl Render for Button {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let button_bounds = Rc::new(Cell::new(Bounds::default()));

    component::button::Button::new("test-button")
      .secondary()
      .rounded_full()
      .icon(IconName::Nixos)
      .on_prepaint({
        let button_bounds = button_bounds.clone();
        move |bounds, _, _| {
          button_bounds.set(bounds);
        }
      })
      .on_click(move |_, window, cx| {
        toggle_panel(ControlPanel, button_bounds.get(), window, cx)
          .expect("Failed to toggle panel");
      })
  }
}
