use gpui_kit::{
  App, Div, InteractiveElement, MouseButton, MouseDownEvent, ParentElement, Styled, Window,
  component::Theme, div,
};

pub fn modal(
  theme: &Theme,
  card: Div,
  on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
  div()
    .on_mouse_down(MouseButton::Left, on_close)
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .rounded_xl()
    .bg(theme.colors.background.opacity(0.8))
    .occlude()
    .child(
      card
        .flex()
        .flex_col()
        .w_3_4()
        .gap_2()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.background)
        // clicks inside the card must not reach the background
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
    )
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, IntoElement, TestAppContext, component::ActiveTheme, px};

  use super::*;
  use crate::test_view;

  #[gpui::test]
  fn renders(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, cx| {
      div()
        .relative()
        .size(px(100.))
        .child(modal(cx.theme(), div(), |_, _, _| {}))
        .into_any_element()
    });
    test_view::draw(handle, cx);
  }
}
