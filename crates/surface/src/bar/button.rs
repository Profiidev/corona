use std::marker::PhantomData;

use gpui_kit::{
  AnyElement, App, Context, ElementId, IntoElement, ParentElement, RenderOnce, Styled, WeakEntity,
  Window,
  base::FocusableExt,
  component::{
    self, ActiveTheme, Icon, Sizable,
    button::{ButtonVariant, ButtonVariants},
  },
  div, px,
};

use crate::{
  bar::{BarState, Widget},
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
  suffix: Option<AnyElement>,
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
      suffix: None,
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

  pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
    self.suffix = Some(suffix.into_any_element());
    self
  }
}

impl<W: Widget, P: Panel> RenderOnce for Button<W, P> {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let grouped = BarState::is_bare(window, cx, self.view.entity_id());
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
      .with_variant(variant(self.danger, grouped))
      .rounded_full()
      .focus_ring(false)
      .with_size(px(ICON_SIZE))
      .p(px(PADDING))
      .cursor_pointer()
      .child(
        div()
          .flex()
          .items_center()
          .gap_1()
          .child(self.icon.with_size(px(ICON_SIZE)))
          .children(self.suffix),
      )
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

/// Danger wins; otherwise no fill of its own inside a group's pill
fn variant(danger: bool, grouped: bool) -> ButtonVariant {
  match (danger, grouped) {
    (true, _) => ButtonVariant::Danger,
    (false, true) => ButtonVariant::Ghost,
    (false, false) => ButtonVariant::Secondary,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn variant_by_danger_then_group() {
    assert_eq!(variant(true, true), ButtonVariant::Danger);
    assert_eq!(variant(true, false), ButtonVariant::Danger);
    assert_eq!(variant(false, true), ButtonVariant::Ghost);
    assert_eq!(variant(false, false), ButtonVariant::Secondary);
  }
}
