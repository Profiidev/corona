use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
};

use crate::ui::{
  app::control_panel::nav::{ControlPanelNav, ControlPanelNavState},
  panel::Panel,
};

pub struct ControlPanel {
  nav_state: Entity<ControlPanelNavState>,
}

impl Panel for ControlPanel {
  const NAME: &'static str = "control_panel";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 600.0;

  fn init(cx: &mut Context<'_, Self>) -> Self {
    let nav_state = cx.new(|_| ControlPanelNavState::new());

    ControlPanel { nav_state }
  }
}

impl Render for ControlPanel {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .p_2()
      .child(ControlPanelNav::new(&self.nav_state))
  }
}
