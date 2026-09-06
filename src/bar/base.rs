use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  AnyView, Bounds, Div, Path, PathBuilder, Pixels, Window, canvas,
  component::{ActiveTheme, *},
  div, point,
  prelude::*,
  px,
};

use crate::config::{
  bar::{BarConfig, WidgetConfig},
  placement::{Placement, PlacementStyle, PlacmentBounds},
};

pub struct Bar {
  placement: Placement,
  height: f32,
  bounds: Rc<Cell<Bounds<Pixels>>>,
  start_widgets: Vec<AnyView>,
  center_widgets: Vec<AnyView>,
  end_widgets: Vec<AnyView>,
}

impl Bar {
  pub fn new(config: BarConfig, cx: &mut Context<Bar>) -> Self {
    let mut init_widgets = |widgets: Vec<WidgetConfig>| {
      widgets
        .into_iter()
        .map(|w| w.widget_type.init(cx))
        .collect::<Vec<_>>()
    };

    let start_widgets = init_widgets(config.start_widgets);
    let center_widgets = init_widgets(config.center_widgets);
    let end_widgets = init_widgets(config.end_widgets);

    Self {
      placement: config.placement,
      height: config.height,
      bounds: Rc::new(Cell::new(Bounds::default())),
      start_widgets,
      center_widgets,
      end_widgets,
    }
  }
}

impl Bar {
  pub fn geometry(&self) -> (Bounds<Pixels>, Placement) {
    (self.bounds.get(), self.placement)
  }
}

impl Render for Bar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme.tokens.status_bar;
    let flare = theme.radius_2xl();

    let viewport = window.viewport_size();
    let len = self.placement.len(viewport);
    window.set_input_region(Some(&[self.placement.rect(
      viewport,
      px(0.),
      len,
      px(self.height),
    )]));

    div()
      .size_full()
      .relative()
      .child(
        canvas(|_, _, _| (), {
          let placement = self.placement;
          move |bounds, _, window, _| {
            if let Some(path) = bar_path(bounds, flare, placement) {
              window.paint_path(path, bg);
            }
          }
        })
        .absolute()
        .size_full()
        .inset_0(),
      )
      .child(
        div()
          .absolute()
          .flex()
          .anchor_p(self.placement)
          .along_start_p(self.placement)
          .along_end_p(self.placement)
          .overflow_hidden()
          .when(self.placement.is_horizontal(), |b| {
            b.flex_row().h(px(self.height)).w_full()
          })
          .when(self.placement.is_vertical(), |b| {
            b.flex_col().w(px(self.height)).h_full()
          })
          .on_prepaint({
            let bar_bounds = self.bounds.clone();
            move |bounds, _, _| {
              bar_bounds.set(bounds);
            }
          })
          .child(widgets(self.start_widgets.clone(), self.placement).justify_start())
          .child(widgets(self.center_widgets.clone(), self.placement).justify_center())
          .child(widgets(self.end_widgets.clone(), self.placement).justify_end()),
      )
  }
}

fn widgets(views: Vec<AnyView>, p: Placement) -> Div {
  div()
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .when(p.is_vertical(), |d| d.flex_col())
    .children(views)
}

fn bar_path(bounds: Bounds<Pixels>, n: Pixels, placement: Placement) -> Option<Path<Pixels>> {
  let (len, depth) = bounds.extent_p(placement);
  let at = |along, across| bounds.point_p(placement, along, across);
  // Mirrored placements reverse the plane, so the arcs sweep the other way.
  let sweep = placement.mirrored();
  let z = px(0.);

  let mut p = PathBuilder::fill();
  p.move_to(at(z, z));
  p.line_to(at(len, z));
  p.line_to(at(len, depth));
  p.arc_to(point(n, n), px(0.), false, sweep, at(len - n, depth - n));
  p.line_to(at(n, depth - n));
  p.arc_to(point(n, n), px(0.), false, sweep, at(z, depth));
  p.close();
  p.build().ok()
}
