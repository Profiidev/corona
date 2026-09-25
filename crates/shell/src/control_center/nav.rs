use std::rc::Rc;

use gpui_kit::{
  App, IntoElement, ParentElement, RenderOnce, Styled, Window,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::variants::ControlCenterType;

#[derive(IntoElement)]
pub struct ControlCenterNav {
  selected: ControlCenterType,
  #[allow(clippy::type_complexity)]
  on_click: Option<Rc<dyn Fn(ControlCenterType, &mut Window, &mut App) + 'static>>,
}

impl ControlCenterNav {
  pub fn new(selected: ControlCenterType) -> Self {
    ControlCenterNav {
      selected,
      on_click: None,
    }
  }

  pub fn on_click<F>(mut self, f: F) -> Self
  where
    F: Fn(ControlCenterType, &mut Window, &mut App) + 'static,
  {
    self.on_click = Some(Rc::new(f));
    self
  }
}

impl RenderOnce for ControlCenterNav {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let on_click = self.on_click;

    div()
      .w(px(48.))
      .h_full()
      .p_2()
      .flex()
      .flex_col()
      .gap_1()
      .rounded_xl()
      .bg(theme.tokens.accent)
      .children(ControlCenterType::iter().map(|v| {
        Button::new(v.as_str())
          .with_variant(if self.selected == v {
            ButtonVariant::Primary
          } else {
            ButtonVariant::Ghost
          })
          .tooltip(v.title())
          .cursor_pointer()
          .icon(v.icon())
          .when_some(on_click.clone(), |button, on_click| {
            button.on_click(move |_, window, cx| on_click(v, window, cx))
          })
      }))
  }
}
