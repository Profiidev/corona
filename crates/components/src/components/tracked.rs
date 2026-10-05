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
