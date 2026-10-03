use std::marker::PhantomData;

use gpui_kit::{
  App, Context, ElementId, IntoElement, ParentElement, RenderOnce, Styled, WeakEntity, Window,
  base::FocusableExt,
  component::{self, ActiveTheme, Icon, Sizable, button::ButtonVariants},
  div,
  prelude::FluentBuilder,
  px,
};

use corona_surface::{
  bar::Widget,
  panel::{Panel, WdigetPanelExt},
};

const ICON_SIZE: f32 = 16.;
const PADDING: f32 = 4.;

#[derive(IntoElement)]
pub struct Button<W: Widget, P: Panel> {
  id: ElementId,
  icon: Icon,
  view: WeakEntity<W>,
  danger: bool,
  dot: bool,
  panel: PhantomData<fn() -> P>,
}

impl<W: Widget, P: Panel> Button<W, P> {
  pub fn new(cx: &mut Context<'_, W>, id: impl Into<ElementId>, icon: impl Into<Icon>) -> Self {
    let view = cx.entity().downgrade();

    Self {
      id: id.into(),
      icon: icon.into(),
      view,
      danger: false,
      dot: false,
      panel: PhantomData,
    }
  }

  pub fn danger(mut self, danger: bool) -> Self {
    self.danger = danger;
    self
  }

  pub fn dot(mut self, dot: bool) -> Self {
    self.dot = dot;
    self
  }
}

impl<W: Widget, P: Panel> RenderOnce for Button<W, P> {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let dot = self.dot.then(|| {
      div()
        .absolute()
        .top_0()
        .right_0()
        .size(px(6.))
        .rounded_full()
        .bg(cx.theme().danger)
    });

    let button = component::button::Button::new(self.id)
      .when_else(self.danger, |b| b.danger(), |b| b.secondary())
      .rounded_full()
      .focus_ring(false)
      .with_size(px(ICON_SIZE))
      .p(px(PADDING))
      .cursor_pointer()
      .child(self.icon.with_size(px(ICON_SIZE)))
      .on_click(move |_, window, cx| {
        self
          .view
          .update(cx, |_, cx| cx.toggle_panel::<P>(window))
          .flatten()
          .expect("Failed to toggle control panel");
      });

    div().relative().child(button).children(dot)
  }
}
