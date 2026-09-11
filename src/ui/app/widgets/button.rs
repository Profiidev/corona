use std::marker::PhantomData;

use gpui_kit::{
  App, Context, ElementId, IntoElement, RenderOnce, Styled, WeakEntity, Window,
  base::FocusableExt,
  component::{self, Icon, Sizable, button::ButtonVariants},
  px,
};

use crate::ui::{
  bar::Widget,
  panel::{Panel, PanelExt},
};

#[derive(IntoElement)]
pub struct Button<W: Widget, P: Panel> {
  id: ElementId,
  icon: Icon,
  view: WeakEntity<W>,
  panel: PhantomData<fn() -> P>,
}

impl<W: Widget, P: Panel> Button<W, P> {
  pub fn new(cx: &mut Context<'_, W>, id: impl Into<ElementId>, icon: impl Into<Icon>) -> Self {
    let view = cx.entity().downgrade();

    Self {
      id: id.into(),
      icon: icon.into(),
      view,
      panel: PhantomData,
    }
  }
}

impl<W: Widget, P: Panel> RenderOnce for Button<W, P> {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    component::button::Button::new(self.id)
      .secondary()
      .rounded_full()
      .focus_ring(false)
      .with_size(px(24.))
      .cursor_pointer()
      .icon(self.icon)
      .on_click(move |_, window, cx| {
        self
          .view
          .update(cx, |_, cx| cx.toggle_panel::<P>(window))
          .flatten()
          .expect("Failed to toggle control panel");
      })
  }
}
