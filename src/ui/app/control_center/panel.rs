use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
};

use crate::ui::{
  app::control_center::{
    layout::ControlCenterLayout,
    nav::{ControlCenterNav, ControlCenterNavState},
  },
  panel::Panel,
};

pub struct ControlCenter {
  nav_state: Entity<ControlCenterNavState>,
}

impl Panel for ControlCenter {
  const NAME: &'static str = "control_panel";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 600.0;

  fn init(cx: &mut Context<'_, Self>) -> Self {
    let nav_state = cx.new(|_| ControlCenterNavState::new());

    ControlCenter { nav_state }
  }
}

impl Render for ControlCenter {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .gap_2()
      .p_2()
      .child(ControlCenterNav::new(&self.nav_state))
      .child(ControlCenterLayout::new(&self.nav_state))
  }
}
