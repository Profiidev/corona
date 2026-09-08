use gpui_kit::{
  App, Context, ElementId, IntoElement, RenderOnce, Styled, WeakEntity, Window,
  base::FocusableExt,
  component::{self, Icon, Sizable, button::ButtonVariants},
  px,
};

use crate::{
  ui::bar::{toggle_panel, widgets::Widget},
  ui::panel::Panel,
};

#[derive(IntoElement)]
pub struct Button<W: Widget, P: Panel, F: FnOnce() -> P + Clone + 'static> {
  id: ElementId,
  icon: Icon,
  panel: F,
  view: WeakEntity<W>,
}

impl<W: Widget, P: Panel, F: FnOnce() -> P + Clone + 'static> Button<W, P, F> {
  pub fn new(
    cx: &mut Context<'_, W>,
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    panel: F,
  ) -> Self {
    let view = cx.entity().downgrade();

    Self {
      id: id.into(),
      icon: icon.into(),
      view,
      panel,
    }
  }
}

impl<W: Widget, P: Panel, F: FnOnce() -> P + Clone + 'static> RenderOnce for Button<W, P, F> {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    component::button::Button::new(self.id)
      .secondary()
      .rounded_full()
      .focus_ring(false)
      .with_size(px(24.))
      .cursor_pointer()
      .icon(self.icon)
      .on_click(move |_, window, cx| {
        let panel = self.panel.clone();
        self
          .view
          .update(cx, |_, cx| toggle_panel(panel, window, cx))
          .flatten()
          .expect("Failed to toggle control panel");
      })
  }
}
