use corona_surface::panel::AppPanelExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, assets::IconName,
  base::StyledExt, component::button::Button, div,
};

use crate::control_center::{ControlCenter, variants::ControlCenterType};

#[derive(IntoElement)]
pub struct ControlCenterLayout {
  selected: ControlCenterType,
  buttons: Vec<AnyElement>,
  content: Option<AnyElement>,
}

impl ControlCenterLayout {
  pub fn new(selected: ControlCenterType) -> Self {
    ControlCenterLayout {
      selected,
      buttons: Vec::new(),
      content: None,
    }
  }

  pub fn buttons(mut self, buttons: impl Iterator<Item = impl IntoElement>) -> Self {
    self.buttons.extend(buttons.map(|b| b.into_any_element()));
    self
  }

  pub fn content(mut self, content: impl IntoElement) -> Self {
    self.content = Some(content.into_any_element());
    self
  }
}

impl RenderOnce for ControlCenterLayout {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .flex_col()
      .child(
        div()
          .flex()
          .items_center()
          .p_2()
          .gap_2()
          .child(
            div()
              .child(self.selected.title())
              .font_bold()
              .text_base()
              .mr_auto(),
          )
          .children(self.buttons)
          .child(
            Button::new("close")
              .icon(IconName::X)
              .cursor_pointer()
              .on_click(|_, _, cx| {
                let _ = cx.close_panel::<ControlCenter>().log_err();
              }),
          ),
      )
      .child(
        div().flex_col().size_full().child(
          self
            .content
            .unwrap_or_else(|| div().child("No content").into_any_element()),
        ),
      )
  }
}
