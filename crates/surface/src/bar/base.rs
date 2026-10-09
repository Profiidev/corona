use std::{
  cell::Cell,
  collections::{HashMap, HashSet},
  rc::Rc,
};

use gpui_kit::{
  AnyView, App, Bounds, Div, EntityId, Path, PathBuilder, Pixels, Window, canvas,
  component::{ActiveTheme, *},
  div, point,
  prelude::*,
  px,
};
use uuid::Uuid;

use crate::bar::state::BarExt;
use corona_components::components::tracked::Tracked;
use corona_config::{
  bar::{BarConfig, WidgetConfig, WidgetEntry},
  placement::{Placement, PlacementStyle, PlacmentBounds},
};

pub struct Bar {
  placement: Placement,
  height: f32,
  background_opacity: f32,
  capsule: bool,
  widget_spacing: f32,
  padding: (f32, f32),
  bounds: Rc<Cell<Bounds<Pixels>>>,
  widget_bounds: HashMap<EntityId, Rc<Cell<Bounds<Pixels>>>>,
  start_widgets: Vec<Entry>,
  center_widgets: Vec<Entry>,
  end_widgets: Vec<Entry>,
  grouped: HashSet<EntityId>,
}

#[derive(Clone)]
enum Entry {
  Widget(AnyView),
  Group(Vec<AnyView>),
}

impl Bar {
  pub fn new(
    config: BarConfig,
    window: &mut Window,
    cx: &mut Context<Bar>,
    display_id: Uuid,
  ) -> Self {
    let mut grouped = HashSet::new();
    let init = |entry: &WidgetEntry, window: &mut Window, cx: &mut Context<Bar>| {
      let data = cx.bar().widget(&entry.widget_type).cloned()?;
      data.init(window, cx, display_id, entry.options.as_ref())
    };
    let mut init_widgets =
      |widgets: Vec<WidgetConfig>, window: &mut Window, cx: &mut Context<Bar>| {
        widgets
          .iter()
          .filter_map(|w| match w {
            WidgetConfig::Widget(entry) => init(entry, window, cx).map(Entry::Widget),
            WidgetConfig::Group { group } => {
              let views: Vec<_> = group.iter().filter_map(|e| init(e, window, cx)).collect();
              grouped.extend(views.iter().map(AnyView::entity_id));
              (!views.is_empty()).then_some(Entry::Group(views))
            }
          })
          .collect::<Vec<_>>()
      };

    let start_widgets = init_widgets(config.start, window, cx);
    let center_widgets = init_widgets(config.center, window, cx);
    let end_widgets = init_widgets(config.end, window, cx);

    Self {
      placement: config.position,
      height: config.thickness,
      background_opacity: config.background_opacity,
      capsule: config.capsule,
      widget_spacing: config.widget_spacing,
      padding: (config.padding_start, config.padding_end),
      bounds: Rc::new(Cell::new(Bounds::default())),
      widget_bounds: HashMap::new(),
      start_widgets,
      center_widgets,
      end_widgets,
      grouped,
    }
  }
}

impl Bar {
  pub fn bounds(&self) -> Bounds<Pixels> {
    self.bounds.get()
  }

  pub fn placement(&self) -> Placement {
    self.placement
  }

  pub fn widget_bounds(&self, widget_id: EntityId) -> Option<Bounds<Pixels>> {
    self.widget_bounds.get(&widget_id).map(|b| b.get())
  }

  /// Drawn without a pill of its own: in a group, or capsules are off
  pub fn is_bare(&self, widget_id: EntityId) -> bool {
    !self.capsule || self.grouped.contains(&widget_id)
  }

  fn widget(&mut self, view: AnyView) -> Tracked {
    let bounds = self
      .widget_bounds
      .entry(view.entity_id())
      .or_default()
      .clone();
    Tracked::new(view, bounds)
  }

  fn widgets(&mut self, entries: Vec<Entry>, cx: &App) -> Div {
    let vertical = self.placement.is_vertical();
    let capsule = self.capsule.then_some(cx.theme().tokens.button_hover);

    div()
      .absolute()
      .inset_0()
      .flex()
      .items_center()
      .gap(px(self.widget_spacing))
      // on every section, so the center one stays centered between the paddings
      .map(|d| {
        let (start, end) = (px(self.padding.0), px(self.padding.1));
        match vertical {
          true => d.flex_col().pt(start).pb(end),
          false => d.pl(start).pr(end),
        }
      })
      .children(entries.into_iter().map(|entry| {
        match entry {
          Entry::Widget(view) => self.widget(view).into_any_element(),
          Entry::Group(views) => div()
            .flex()
            .items_center()
            .when(vertical, |d| d.flex_col())
            .rounded_full()
            .when_some(capsule, |d, capsule| d.bg(capsule))
            .children(views.into_iter().map(|v| self.widget(v)))
            .into_any_element(),
        }
      }))
  }
}

#[cfg(test)]
impl Bar {
  /// The views of the start, center and end sections; a group is one entry
  pub(crate) fn sections(&self) -> [Vec<Vec<AnyView>>; 3] {
    [&self.start_widgets, &self.center_widgets, &self.end_widgets].map(|entries| {
      entries
        .iter()
        .map(|entry| match entry {
          Entry::Widget(view) => vec![view.clone()],
          Entry::Group(views) => views.clone(),
        })
        .collect()
    })
  }
}

impl Render for Bar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme.tokens.background.opacity(self.background_opacity);
    let flare = theme.radius * 2;

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
        // the canvas above paints the background; painting it here too would
        // show as a darker band once it is translucent
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
          .child(self.widgets(self.start_widgets.clone(), cx).justify_start())
          .child(
            self
              .widgets(self.center_widgets.clone(), cx)
              .justify_center(),
          )
          .child(self.widgets(self.end_widgets.clone(), cx).justify_end()),
      )
  }
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

#[cfg(test)]
mod tests {
  use gpui_kit::size;

  use super::*;

  const ALL: [Placement; 4] = [
    Placement::Top,
    Placement::Bottom,
    Placement::Left,
    Placement::Right,
  ];

  #[test]
  fn path_fills_its_bounds_on_every_side() {
    let bounds = Bounds::new(point(px(5.), px(5.)), size(px(400.), px(40.)));
    for placement in ALL {
      let path = bar_path(bounds, px(8.), placement).expect("a path");
      let b = path.bounds;
      let e = px(0.5);
      assert!(b.left() >= bounds.left() - e && b.right() <= bounds.right() + e);
      assert!(b.top() >= bounds.top() - e && b.bottom() <= bounds.bottom() + e);
    }
  }

  #[test]
  fn zero_sized_bounds_do_not_panic() {
    for placement in ALL {
      let _ = bar_path(Bounds::default(), px(8.), placement);
      let _ = bar_path(Bounds::default(), px(0.), placement);
    }
  }
}
