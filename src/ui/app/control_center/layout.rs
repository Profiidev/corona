use gpui_kit::{
  AnyElement, App, Entity, IntoElement, ParentElement, RenderOnce, Styled, Window,
  assets::IconName, base::StyledExt, component::button::Button, div,
};

use crate::{
  error::ErrorLogExt,
  ui::{
    app::control_center::{ControlCenter, nav::ControlCenterNavState},
    panel::AppPanelExt,
  },
};

#[derive(IntoElement)]
pub struct ControlCenterLayout {
  state: Entity<ControlCenterNavState>,
  buttons: Vec<AnyElement>,
  content: Option<AnyElement>,
}

impl ControlCenterLayout {
  pub fn new(title: &Entity<ControlCenterNavState>) -> Self {
    ControlCenterLayout {
      state: title.clone(),
      buttons: Vec::new(),
      content: None,
    }
  }

  pub fn button(mut self, button: impl IntoElement) -> Self {
    self.buttons.push(button.into_any_element());
    self
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
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let state = self.state.read(cx);

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
              .child(state.selected.title())
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
        div().flex_col().size_full().p_2().child(
          self
            .content
            .unwrap_or_else(|| div().child("No content").into_any_element()),
        ),
      )
  }
}
