use corona_surface::panel::Panel;
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
};

use crate::control_center::{
  ControlCenterPanel, ControlCenterPanelHandle, layout::ControlCenterLayout, nav::ControlCenterNav,
  variants::ControlCenterType,
};

pub struct ControlCenter {
  selected: ControlCenterType,
  panel: Box<dyn ControlCenterPanelHandle>,
}

impl Panel for ControlCenter {
  const NAME: &'static str = "control_center";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 500.0;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let selected = ControlCenterType::Dashboard;

    ControlCenter {
      panel: selected.handle(window, cx),
      selected,
    }
  }
}

pub struct Standalone<T: ControlCenterPanel>(Entity<T>);

impl<T: ControlCenterPanel> Panel for Standalone<T> {
  const NAME: &'static str = T::TYPE.as_str();
  const WIDTH: f32 = ControlCenter::WIDTH - 8.0 - ControlCenterNav::WIDTH;
  const HEIGHT: f32 = <T as ControlCenterPanel>::HEIGHT;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    Self(cx.new(|cx| T::init(window, cx)))
  }
}

impl<T: ControlCenterPanel> Render for Standalone<T> {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div().size_full().p_2().child(
      ControlCenterLayout::new(T::TYPE)
        .closes::<Self>()
        .buttons(self.0.buttons(cx).into_iter())
        .content(self.0.clone()),
    )
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
