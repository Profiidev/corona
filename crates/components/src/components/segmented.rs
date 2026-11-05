use std::rc::Rc;

use gpui_kit::{
  App, Div, ElementId, ParentElement, SharedString, Styled, Window,
  component::{
    Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};

/// A row of equally wide buttons picking one of `items`, as tab bars and filters do. Each item is
/// its value, button id and label; `on_select` gets the clicked value
pub fn segmented<T: Copy + PartialEq + 'static>(
  items: impl IntoIterator<Item = (T, impl Into<ElementId>, impl Into<SharedString>)>,
  selected: T,
  on_select: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> Div {
  let on_select = Rc::new(on_select);
  div()
    .flex()
    .flex_shrink_0()
    .gap_1()
    .children(items.into_iter().map(|(value, id, label)| {
      let on_select = on_select.clone();
      Button::new(id)
        .label(label)
        .small()
        .flex_1()
        .cursor_pointer()
        .when(value == selected, |b| b.primary())
        .on_click(move |_, window, cx| on_select(&value, window, cx))
    }))
}
