use std::fmt::{Debug, Display};

use corona_config::ConfigProvider;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, ClickEvent, Context, ElementId, IntoElement, ParentElement, RenderOnce, Styled, Window,
  assets::IconName,
  component::{ActiveTheme, Sizable, button::Button},
  div,
  prelude::FluentBuilder,
};

pub trait CardExt: Styled + Sized {
  fn card(self, cx: &App) -> Self {
    let theme = cx.theme();
    // kept, but invisible, when off, so content does not shift
    let border = match cx.config().theme.card_borders {
      true => theme.border,
      false => gpui_kit::transparent_black(),
    };
    self
      .rounded_xl()
      .bg(theme.colors.accent)
      .border_color(border)
      .border_1()
  }
}

impl<T: Styled> CardExt for T {}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct ErrorCard {
  id: ElementId,
  message: String,
  on_dismiss: Option<ClickHandler>,
}

impl ErrorCard {
  pub fn new(id: impl Into<ElementId>, message: impl Into<String>) -> Self {
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
      .card(cx)
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

/// The error a page shows in a dismissable card
#[derive(Default)]
pub struct PageError(Option<String>);

impl PageError {
  /// Logs what `result` failed with and shows it; a success leaves the shown error
  pub fn set<E: Debug + Display>(&mut self, result: Result<(), E>) {
    if let Err(e) = result.log_err() {
      self.0 = Some(e.to_string());
    }
  }

  pub fn show(&mut self, message: impl Into<String>) {
    self.0 = Some(message.into());
  }

  pub fn clear(&mut self) {
    self.0 = None;
  }

  pub fn message(&self) -> Option<&str> {
    self.0.as_deref()
  }

  /// The card, if there is an error; dismissing it clears the error `field` points to
  pub fn card<T: 'static>(
    &self,
    id: impl Into<ElementId>,
    field: fn(&mut T) -> &mut Self,
    cx: &Context<T>,
  ) -> Option<ErrorCard> {
    let error = self.0.clone()?;
    Some(
      ErrorCard::new(id, error).on_dismiss(cx.listener(move |this, _, _, cx| {
        field(this).clear();
        cx.notify();
      })),
    )
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext, px};

  use super::*;
  use crate::test_view;

  #[gpui::test]
  fn cards_render_with_and_without_borders(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, cx| {
      div()
        .child(div().size(px(10.)).card(cx))
        .child(ErrorCard::new("e1", "went wrong"))
        .child(ErrorCard::new("e2", "dismissable").on_dismiss(|_, _, _| {}))
        .into_any_element()
    });
    test_view::draw(handle, cx);
    cx.update(|cx| cx.global_mut::<corona_config::Config>().theme.card_borders = false);
    test_view::draw(handle, cx);
  }

  #[test]
  fn page_error_keeps_the_last_failure() {
    let mut error = PageError::default();
    error.set(Err("failed"));
    error.set(Ok::<_, &str>(()));
    assert_eq!(error.message(), Some("failed"));
    error.clear();
    assert_eq!(error.message(), None);
  }
}
