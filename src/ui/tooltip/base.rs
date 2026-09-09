use gpui_kit::{
  AnyView, Context, IntoElement, ParentElement, Render, Styled, Window, component::ActiveTheme,
  div, px,
};

pub const BORDER: f32 = 2.;

pub struct BaseTooltip {
  tooltip: AnyView,
}

impl BaseTooltip {
  pub fn new(tooltip: AnyView) -> Self {
    Self { tooltip }
  }

  pub fn show(&mut self, tooltip: AnyView, cx: &mut Context<'_, BaseTooltip>) {
    self.tooltip = tooltip;
    cx.notify();
  }
}

impl Render for BaseTooltip {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .size_full()
      .bg(theme.tokens.background)
      .rounded(theme.radius_2xl() / 2.)
      .border_color(theme.tokens.button_hover)
      .border(px(BORDER))
      .overflow_hidden()
      .child(self.tooltip.clone())
  }
}
