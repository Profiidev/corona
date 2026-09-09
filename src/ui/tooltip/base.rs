use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  AnyView, Bounds, Context, IntoElement, ParentElement, Pixels, Render, Size, Styled, Window,
  base::ElementExt, component::ActiveTheme, div, px,
};

use crate::{
  config::placement::{Placement, PlacementStyle},
  ui::tooltip::align::Align,
};

const BORDER: f32 = 2.;

pub struct BaseTooltip {
  tooltip: AnyView,
  align: Align,
  size: Size<Pixels>,
  placement: Placement,
  bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl BaseTooltip {
  pub fn new(tooltip: AnyView, align: Align, size: Size<Pixels>, placement: Placement) -> Self {
    Self {
      tooltip,
      align,
      size,
      placement,
      bounds: Rc::new(Cell::new(Bounds::default())),
    }
  }

  pub fn show(
    &mut self,
    tooltip: AnyView,
    align: Align,
    size: Size<Pixels>,
    cx: &mut Context<'_, BaseTooltip>,
  ) {
    self.tooltip = tooltip;
    self.align = align;
    self.size = size;
    cx.notify();
  }
}

impl Render for BaseTooltip {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme.tokens.background;
    let radius = theme.radius_lg;

    let bounds = self.bounds.clone();
    window.set_input_region(Some(&[self.bounds.get()]));

    div().size_full().bg(gpui_kit::transparent_black()).child(
      div()
        .absolute()
        .across_p(self.placement, px(self.align.across))
        .along_p(self.placement, px(self.align.along))
        .w(self.size.width + px(BORDER * 2.))
        .h(self.size.height + px(BORDER * 2.))
        .bg(bg)
        .rounded(radius)
        .border_color(theme.tokens.button_hover)
        .border(px(BORDER))
        .overflow_hidden()
        .on_prepaint(move |b, _, _| bounds.set(b))
        .child(self.tooltip.clone()),
    )
  }
}
