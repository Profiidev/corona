use std::sync::Arc;

use gpui_kit::{App, Dmabuf, IntoElement, RenderOnce, StyleRefinement, Styled, Window, canvas};

/// Draws a captured frame straight from its GPU memory, stretched to the
/// element's bounds. Size it like any element, e.g. `.size_full()`.
#[derive(IntoElement)]
pub struct FrameView {
  surface: Arc<Dmabuf>,
  style: StyleRefinement,
}

impl FrameView {
  pub fn new(surface: Arc<Dmabuf>) -> Self {
    Self {
      surface,
      style: StyleRefinement::default(),
    }
  }
}

impl Styled for FrameView {
  fn style(&mut self) -> &mut StyleRefinement {
    &mut self.style
  }
}

impl RenderOnce for FrameView {
  fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
    let surface = self.surface;
    let mut canvas = canvas(
      |_, _, _| (),
      move |bounds, _, window, _| window.paint_dmabuf(bounds, surface),
    );
    *canvas.style() = self.style;
    canvas
  }
}
