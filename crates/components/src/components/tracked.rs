use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
  LayoutId, Pixels, Window,
};

pub struct Tracked {
  child: AnyElement,
  bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl Tracked {
  pub fn new(child: impl IntoElement, bounds: Rc<Cell<Bounds<Pixels>>>) -> Self {
    Self {
      child: child.into_any_element(),
      bounds,
    }
  }
}

impl IntoElement for Tracked {
  type Element = Self;

  fn into_element(self) -> Self {
    self
  }
}

impl Element for Tracked {
  type RequestLayoutState = ();
  type PrepaintState = ();

  fn id(&self) -> Option<ElementId> {
    None
  }

  fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
    None
  }

  fn request_layout(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    window: &mut Window,
    cx: &mut App,
  ) -> (LayoutId, ()) {
    (self.child.request_layout(window, cx), ())
  }

  fn prepaint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    _: &mut (),
    window: &mut Window,
    cx: &mut App,
  ) {
    self.bounds.set(bounds);
    self.child.prepaint(window, cx);
  }

  fn paint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    _: Bounds<Pixels>,
    _: &mut (),
    _: &mut (),
    window: &mut Window,
    cx: &mut App,
  ) {
    self.child.paint(window, cx);
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, ParentElement, Styled, TestAppContext, div, px};

  use super::*;
  use crate::test_view;

  #[gpui::test]
  fn records_the_child_bounds(cx: &mut TestAppContext) {
    let bounds = Rc::new(Cell::new(Bounds::default()));
    let b = bounds.clone();
    let (handle, _) = test_view::open(cx, move |_, _| {
      div()
        .pl(px(10.))
        .child(Tracked::new(div().w(px(30.)).h(px(20.)), b.clone()))
        .into_any_element()
    });
    test_view::draw(handle, cx);
    let got = bounds.get();
    assert_eq!(got.origin.x, px(10.));
    assert_eq!(got.size.width, px(30.));
    assert_eq!(got.size.height, px(20.));
  }
}
