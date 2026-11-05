use gpui_kit::{
  App, Div, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
  },
  div, px,
};

/// The options and remove buttons of a row
pub fn row_buttons(id: impl Fn(&str) -> String, has_options: bool) -> (Option<Button>, Button) {
  let options = has_options.then(|| {
    Button::new(id("options"))
      .icon(IconName::Settings2)
      .cursor_pointer()
      .ghost()
      .xsmall()
  });
  let remove = Button::new(id("remove"))
    .icon(IconName::X)
    .cursor_pointer()
    .ghost()
    .xsmall();
  (options, remove)
}

/// How a widget's row looks, shared by the row and its drag preview
pub fn row_body(
  label: impl IntoElement,
  options: Option<impl IntoElement>,
  remove: Button,
  cx: &App,
) -> Div {
  let theme = cx.theme();
  div()
    .flex()
    .items_center()
    .gap_1()
    .px_1()
    .min_h(px(28.))
    .rounded(theme.radius)
    .bg(theme.colors.background)
    .text_color(theme.foreground)
    .child(
      Icon::new(IconName::GripVertical)
        .xsmall()
        .text_color(theme.muted_foreground),
    )
    .child(div().flex_1().min_w_0().truncate().text_sm().child(label))
    .children(options)
    .child(remove)
}
