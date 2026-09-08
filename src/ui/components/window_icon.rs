use gpui_kit::{
  AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div, img,
  prelude::FluentBuilder, px, relative,
};

use crate::integration::desktop::entry::icon_for_class_or_default;

const ICON_SIZE: u16 = 18;

#[derive(IntoElement)]
pub struct WindowIcon {
  class: String,
  size: u16,
  children: Vec<AnyElement>,
}

impl WindowIcon {
  pub fn new(class: impl Into<String>) -> Self {
    Self {
      class: class.into(),
      size: ICON_SIZE,
      children: Vec::new(),
    }
  }

  pub fn size(mut self, size: u16) -> Self {
    self.size = size;
    self
  }
}

impl ParentElement for WindowIcon {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl RenderOnce for WindowIcon {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let icon = icon_for_class_or_default(&self.class, self.size);

    div()
      .flex()
      .items_center()
      .justify_center()
      .relative()
      .h(px(self.size as f32))
      .w(px(self.size as f32))
      .rounded_full()
      .map(|this| match icon {
        Some(path) => this.child(img(path).size_full()),
        None => this
          .text_size(px(10.))
          .line_height(relative(1.))
          .child(self.class.chars().next().unwrap_or('?').to_string()),
      })
      .children(self.children)
  }
}
