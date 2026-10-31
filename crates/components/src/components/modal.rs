use std::rc::Rc;

use gpui_kit::{
  AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, MouseButton,
  MouseDownEvent, ParentElement, RenderOnce, SharedString, Styled, Window,
  assets::IconName,
  component::{
    ActiveTheme, Icon, Sizable, Theme,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;

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

type OnCancel = Rc<dyn Fn(&(), &mut Window, &mut App)>;
type OnConfirm = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

struct Confirm {
  label: SharedString,
  danger: bool,
  busy: bool,
  on_click: OnConfirm,
}

/// A modal card: a bold title over a muted message, its children, an error, then Cancel and the
/// confirm button on the right
#[derive(IntoElement)]
pub struct Dialog {
  /// prefixes the buttons' ids
  id: SharedString,
  icon: Option<IconName>,
  title: SharedString,
  message: Option<SharedString>,
  body: Vec<AnyElement>,
  error: Option<SharedString>,
  cancel_label: Option<SharedString>,
  on_cancel: OnCancel,
  confirm: Option<Confirm>,
}

impl Dialog {
  /// `on_cancel` runs on Cancel and on a click beside the card
  pub fn new(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    on_cancel: impl Fn(&(), &mut Window, &mut App) + 'static,
  ) -> Self {
    Self {
      id: id.into(),
      icon: None,
      title: title.into(),
      message: None,
      body: Vec::new(),
      error: None,
      cancel_label: None,
      on_cancel: Rc::new(on_cancel),
      confirm: None,
    }
  }

  /// Shown left of the title, the message then under the title
  pub fn icon(mut self, icon: IconName) -> Self {
    self.icon = Some(icon);
    self
  }

  pub fn message(mut self, message: impl Into<SharedString>) -> Self {
    self.message = Some(message.into());
    self
  }

  pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
    self.error = error.map(Into::into);
    self
  }

  pub fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
    self.cancel_label = Some(label.into());
    self
  }

  /// The confirm button; while `busy` it spins and a click beside the card does not cancel
  pub fn confirm(
    mut self,
    label: impl Into<SharedString>,
    danger: bool,
    busy: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    self.confirm = Some(Confirm {
      label: label.into(),
      danger,
      busy,
      on_click: Box::new(on_click),
    });
    self
  }
}

impl ParentElement for Dialog {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.body.extend(elements);
  }
}

impl RenderOnce for Dialog {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let title = div()
      .font_weight(gpui_kit::FontWeight::BOLD)
      .text_sm()
      .truncate()
      .child(self.title);
    let message = self.message.map(|message| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(message)
    });
    let busy = self.confirm.as_ref().is_some_and(|c| c.busy);
    let on_cancel = self.on_cancel.clone();
    let card = match self.icon {
      Some(icon) => div().child(
        div()
          .flex()
          .items_center()
          .gap_2()
          .child(Icon::new(icon).flex_shrink_0())
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(title)
              .children(message),
          ),
      ),
      None => div().child(title).children(message),
    };
    modal(
      theme,
      card
        .max_h_full()
        .children(self.body)
        .when_some(self.error, |d, error| {
          d.child(div().text_xs().text_color(theme.colors.danger).child(error))
        })
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new(ElementId::from(format!("{}-cancel", self.id)))
                .label(
                  self
                    .cancel_label
                    .unwrap_or_else(|| t!("app.common.cancel").into()),
                )
                .small()
                .cursor_pointer()
                .on_click(move |_, window, cx| on_cancel(&(), window, cx)),
            )
            .when_some(self.confirm, |d, confirm| {
              d.child(
                Button::new(ElementId::from(format!("{}-confirm", self.id)))
                  .label(confirm.label)
                  .small()
                  .loading(confirm.busy)
                  .when_else(confirm.danger, |b| b.danger(), |b| b.primary())
                  .cursor_pointer()
                  .on_click(confirm.on_click),
              )
            }),
        ),
      move |_, window, cx| {
        if !busy {
          (self.on_cancel)(&(), window, cx);
        }
      },
    )
  }
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

  #[gpui::test]
  fn dialog_renders(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, _| {
      div()
        .relative()
        .size(px(200.))
        .child(
          Dialog::new("plain", "title", |_, _, _| {})
            .message("message")
            .child(div())
            .error(Some("failed"))
            .confirm("ok", false, false, |_, _, _| {}),
        )
        .child(
          Dialog::new("icon", "title", |_, _, _| {})
            .icon(IconName::Eject)
            .cancel_label("close")
            .confirm("delete", true, true, |_, _, _| {}),
        )
        .into_any_element()
    });
    test_view::draw(handle, cx);
  }
}
