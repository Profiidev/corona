use gpui_kit::{
  App, Entity, IntoElement, ParentElement, RenderOnce, Styled, Window,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div, px,
};

use crate::ui::app::control_center::variants::ControlCenterType;

#[derive(IntoElement)]
pub struct ControlCenterNav {
  state: Entity<ControlCenterNavState>,
}

pub struct ControlCenterNavState {
  pub selected: ControlCenterType,
}

impl ControlCenterNav {
  pub fn new(state: &Entity<ControlCenterNavState>) -> Self {
    ControlCenterNav {
      state: state.clone(),
    }
  }
}

impl ControlCenterNavState {
  pub fn new() -> Self {
    ControlCenterNavState {
      selected: ControlCenterType::Dashboard,
    }
  }
}

impl RenderOnce for ControlCenterNav {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let selected = self.state.read(cx).selected;

    div()
      .w(px(48.))
      .h_full()
      .p_2()
      .flex()
      .flex_col()
      .gap_1()
      .rounded_2xl()
      .bg(theme.tokens.sidebar)
      .children(ControlCenterType::iter().map(|v| {
        Button::new(v.as_str())
          .with_variant(if selected == v {
            ButtonVariant::Primary
          } else {
            ButtonVariant::Ghost
          })
          .tooltip(v.title())
          .cursor_pointer()
          .icon(v.icon())
          .on_click({
            let state = self.state.clone();
            move |_, _, cx| {
              state.update(cx, |this, cx| {
                this.selected = v;
                cx.notify();
              });
            }
          })
      }))
  }
}
