use gpui_kit::{
  App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
  assets::IconName,
  component::{ActiveTheme, Sizable, Theme, button::Button},
  div,
  prelude::FluentBuilder,
};

pub trait CardExt: Styled + Sized {
  fn card(self, theme: &Theme) -> Self {
    self
      .rounded_xl()
      .bg(theme.colors.accent)
      .border_color(theme.border)
      .border_1()
  }
}

impl<T: Styled> CardExt for T {}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ErrorCard {
  id: ElementId,
  message: SharedString,
  on_dismiss: Option<ClickHandler>,
}

impl ErrorCard {
  pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
    Self {
      id: id.into(),
      message: message.into(),
      on_dismiss: None,
    }
  }

  pub fn on_dismiss(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
    self.on_dismiss = Some(Box::new(f));
    self
  }
}

impl RenderOnce for ErrorCard {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .flex()
      .gap_2()
      .p_2()
      .card(theme)
      .child(
        div()
          .text_sm()
          .text_color(theme.colors.danger)
          .truncate()
          .child(self.message),
      )
      .when_some(self.on_dismiss, |d, on_dismiss| {
        d.child(
          Button::new(self.id)
            .small()
            .ml_auto()
            .icon(IconName::X)
            .cursor_pointer()
            .on_click(on_dismiss),
        )
      })
  }
}
