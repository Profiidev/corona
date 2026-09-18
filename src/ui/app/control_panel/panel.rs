use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Window,
  assets::IconName,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div, px,
};

use crate::ui::panel::Panel;

pub struct ControlPanel {
  panel: String,
}

impl Panel for ControlPanel {
  const NAME: &'static str = "control_panel";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 600.0;

  fn init(_cx: &mut Context<'_, Self>) -> Self {
    ControlPanel {
      panel: "root".to_string(),
    }
  }
}

const NAV_ITEMS: &[(&str, IconName)] = &[
  ("root", IconName::LayoutDashboard),
  ("audio", IconName::Volume2),
  ("video", IconName::Video),
  ("network", IconName::Wifi),
  ("system", IconName::Settings),
];

impl Render for ControlPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div().size_full().p_2().child(
      div()
        .w(px(48.))
        .h_full()
        .p_2()
        .flex()
        .flex_col()
        .gap_1()
        .rounded_2xl()
        .bg(theme.tokens.sidebar)
        .children(NAV_ITEMS.iter().map(|&(id, icon)| {
          Button::new(id)
            .with_variant(if self.panel == id {
              ButtonVariant::Primary
            } else {
              ButtonVariant::Ghost
            })
            .cursor_pointer()
            .icon(icon)
            .on_click(cx.listener(|this, _, _, cx| {
              this.panel = id.to_string();
              cx.notify();
            }))
        })),
    )
  }
}
