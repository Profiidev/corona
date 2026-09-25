use corona_surface::panel::Panel;
use gpui_kit::{Context, IntoElement, ParentElement, Render, Styled, Window, div};

use crate::control_center::{
  ControlCenterPanelHandle, layout::ControlCenterLayout, nav::ControlCenterNav,
  variants::ControlCenterType,
};

pub struct ControlCenter {
  selected: ControlCenterType,
  panel: Box<dyn ControlCenterPanelHandle>,
}

impl Panel for ControlCenter {
  const NAME: &'static str = "control_panel";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 600.0;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let selected = ControlCenterType::Dashboard;

    ControlCenter {
      panel: selected.handle(window, cx),
      selected,
    }
  }
}

impl Render for ControlCenter {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .gap_2()
      .p_2()
      .child(ControlCenterNav::new(self.selected).on_click({
        let handle = cx.entity().downgrade();
        move |new, window, cx| {
          let _ = handle.update(cx, |this, cx| {
            this.selected = new;
            this.panel = new.handle(window, cx);
            cx.notify();
          });
        }
      }))
      .child(
        ControlCenterLayout::new(self.selected)
          .buttons(self.panel.buttons(cx).into_iter())
          .content(self.panel.view()),
      )
  }
}
