use std::time::Duration;

use corona_components::animation::{animation_duration, smooth_retarget::SmoothRetarget};
use corona_config::{
  APP_NAME, ConfigProvider,
  placement::{Placement, PlacementStyle, PlacmentBounds},
};
use gpui_kit::{
  AnyView, AnyWindowHandle, AppContext, Background, Bounds, Canvas, Context, DisplayId,
  InteractiveElement, IntoElement, MouseButton, ParentElement, Path, PathBuilder, Pixels, Render,
  Size, Styled, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
  WindowKind, WindowOptions, canvas,
  component::ActiveTheme,
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::FluentBuilder,
  px,
};

use crate::panel::{
  PANEL_NAME, align::Align, state::PanelState, style::PanelStyle, variants::PanelData,
};

const PANEL_OPEN_SPEED: Duration = Duration::from_millis(250);

pub struct BasePanel {
  name: String,
  panel: AnyView,
  width: f32,
  height: f32,
  align: Align,
  placement: Placement,
  // True when opening or open, false when closing. Destroyed when closed.
  open: bool,
  blocks_input: bool,
  removing: bool,
  anim: SmoothRetarget,
  display: Option<DisplayId>,
  blockers: Vec<AnyWindowHandle>,
}

impl BasePanel {
  pub fn new(
    data: &PanelData,
    align: Align,
    placement: Placement,
    display: Option<DisplayId>,
    window: &mut Window,
    cx: &mut Context<'_, BasePanel>,
  ) -> Self {
    // Opening windows while this one is still being built is not allowed.
    cx.spawn(async move |this, cx| this.update(cx, |this, cx| this.block(cx)))
      .detach();

    PanelState::mark_open(&data.name, true, cx);
    Self {
      name: data.name.clone(),
      display,
      blockers: Vec::new(),
      panel: data.init(window, cx),
      width: data.width,
      height: data.height,
      open: true,
      blocks_input: true,
      removing: false,
      anim: SmoothRetarget::new(0.),
      align,
      placement,
    }
  }

  pub fn close(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = false;
    PanelState::mark_open(&self.name, false, cx);
    let blockers = std::mem::take(&mut self.blockers);
    cx.defer(move |cx| {
      for handle in blockers {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
      }
    });
    cx.notify();
  }

  pub fn open(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = true;
    PanelState::mark_open(&self.name, true, cx);
    self.block(cx);
    cx.notify();
  }

  fn block(&mut self, cx: &mut Context<'_, BasePanel>) {
    if !self.open || !self.blockers.is_empty() {
      return;
    }
    let panel = cx.weak_entity();
    for id in cx.displays().iter().map(|d| d.id()) {
      if Some(id) == self.display {
        continue;
      }
      let panel = panel.clone();
      match cx.open_window(blocker_options(id), |_, cx| cx.new(|_| Blocker { panel })) {
        Ok(handle) => self.blockers.push(handle.into()),
        Err(e) => tracing::warn!("panel: no click catcher on {id:?}: {e:#}"),
      }
    }
  }

  pub fn is_open(&self) -> bool {
    self.open
  }

  pub fn align(&self) -> Align {
    self.align
  }
}

impl Render for BasePanel {
  fn render(
    &mut self,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::prelude::Context<Self>,
  ) -> impl gpui_kit::prelude::IntoElement {
    let theme = cx.theme();
    let bg = theme
      .tokens
      .background
      .opacity(cx.config().control_center.background_opacity);
    let br = theme.panel_radius();

    let (bn, nl, nr) = if self.align == Align::Left {
      (br, 0., 1.)
    } else if self.align == Align::Right {
      (br, 1., 0.)
    } else {
      (0., 1., 1.)
    };

    let speed = animation_duration(PANEL_OPEN_SPEED, cx);
    self.anim.retarget(if self.open { 1. } else { 0. }, speed);
    let (progress, animating) = self.anim.value();
    if animating {
      window.request_animation_frame();
    }
    let h = (self.height + bn) * progress;

    if self.open {
      if !self.blocks_input {
        self.blocks_input = true;
        window.set_input_region(None);
      }
    } else {
      self.blocks_input = false;
      let viewport = window.viewport_size();
      let along = match self.align {
        Align::Left => 0.,
        Align::Relative(x) => x - self.width / 2.,
        Align::Right if self.placement.is_horizontal() => viewport.width.as_f32() - self.width,
        Align::Right => viewport.height.as_f32() - self.width,
      };
      let panel = self
        .placement
        .rect(viewport, px(along), px(self.width), px(h - bn));
      window.set_input_region(Some(&[panel]));
    }

    if !self.open && progress == 0. && !self.removing {
      self.removing = true;
      let handle = window.window_handle();
      cx.spawn(async move |_, cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
      })
      .detach();
    }

    div()
      .size_full()
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, _, _, cx| this.close(cx)),
      )
      .child(
        div()
          // Swallows clicks so they don't reach the dismiss handler above.
          .occlude()
          .absolute()
          .anchor_p(self.placement)
          .map(|d| match self.align {
            Align::Left => d.along_start_p(self.placement),
            Align::Relative(x) => d.along_p(self.placement, px(x - self.width / 2.)),
            Align::Right => d.along_end_p(self.placement),
          })
          .size_p(self.placement, px(self.width + br * (nl + nr)), px(h))
          .child(
            panel_shape(br, self.align, self.placement, bg)
              .absolute()
              .size_full()
              .inset_0(),
          )
          .child(
            div()
              .absolute()
              .bg(gpui_kit::transparent_black())
              .anchor_p(self.placement)
              .along_p(self.placement, px(br * nl))
              .size_p(self.placement, px(self.width), px(h - bn))
              .overflow_hidden()
              .child(self.panel.clone()),
          ),
      )
  }
}

struct Blocker {
  panel: WeakEntity<BasePanel>,
}

impl Render for Blocker {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div().size_full().on_mouse_down(
      MouseButton::Left,
      cx.listener(|this, _, _, cx| {
        if let Some(panel) = this.panel.upgrade() {
          panel.update(cx, |panel, cx| panel.close(cx));
        }
      }),
    )
  }
}

fn blocker_options(display_id: DisplayId) -> WindowOptions {
  WindowOptions {
    kind: WindowKind::LayerShell(LayerShellOptions {
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
      exclusive_zone: None,
      exclusive_edge: None,
      margin: None,
      layer: Layer::Top,
      namespace: format!("{PANEL_NAME}_blocker"),
      keyboard_interactivity: KeyboardInteractivity::None,
    }),
    window_background: WindowBackgroundAppearance::Transparent,
    window_decorations: Some(WindowDecorations::Client),
    inactive_frame_interval: None,
    app_id: Some(APP_NAME.to_string()),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size: Size::new(px(0.), px(0.)),
    })),
    display_id: Some(display_id),
    ..Default::default()
  }
}

pub fn panel_shape(
  radius: f32,
  align: Align,
  placement: Placement,
  color: impl Into<Background> + 'static,
) -> Canvas<()> {
  canvas(
    |_, _, _| (),
    move |bounds, _, window, _| {
      if let Some(path) = panel_path(bounds, px(radius), align, placement) {
        window.paint_path(path, color);
      }
    },
  )
}

pub fn panel_path(
  bounds: Bounds<Pixels>,
  n: Pixels,
  align: Align,
  placement: Placement,
) -> Option<Path<Pixels>> {
  let (len, depth) = bounds.extent_p(placement);
  let at = |along, across| bounds.point_p(placement, along, across);
  // Mirrored placements reverse the plane, so every arc sweeps the other way.
  let s = placement.mirrored();
  // Radii are extents, so the axes swap but nothing offsets.
  let r = |along, across| placement.vec(along, across);
  let (h, nf) = (depth.as_f32(), n.as_f32());
  let z = px(0.);

  // Space left for the notch on the side wall.
  let bnf = if !matches!(align, Align::Relative(_)) {
    nf.min(h)
  } else {
    0.
  };

  let ny = px(nf.min((h - bnf) / 2.));
  let bn = px(bnf);

  let mut p = PathBuilder::fill();
  p.move_to(at(z, z));
  p.line_to(at(len, z));

  if !matches!(align, Align::Right) {
    p.arc_to(r(n, ny), px(0.), false, s, at(len - n, ny));
    p.line_to(at(len - n, depth - ny - bn));
    p.arc_to(r(n, ny), px(0.), false, !s, at(len - n - n, depth - bn));
  } else {
    p.line_to(at(len, depth));
    p.arc_to(r(n, n), px(0.), false, s, at(len - n, depth - bn));
  }

  if !matches!(align, Align::Left) {
    p.line_to(at(n + n, depth - bn));
    p.arc_to(r(n, ny), px(0.), false, !s, at(n, depth - ny - bn));
    p.line_to(at(n, ny));
    p.arc_to(r(n, ny), px(0.), false, s, at(z, z));
  } else {
    p.line_to(at(n, depth - bn));
    p.arc_to(r(n, n), px(0.), false, s, at(z, depth));
    p.line_to(at(z, z));
  }

  p.close();
  p.build().ok()
}

#[cfg(test)]
mod tests {
  use gpui_kit::size;

  use super::*;

  pub(crate) const ALL: [Placement; 4] = [
    Placement::Top,
    Placement::Bottom,
    Placement::Left,
    Placement::Right,
  ];

  fn inside(path: &Path<Pixels>, bounds: Bounds<Pixels>) -> bool {
    let e = px(0.5);
    let b = path.bounds;
    b.left() >= bounds.left() - e
      && b.top() >= bounds.top() - e
      && b.right() <= bounds.right() + e
      && b.bottom() <= bounds.bottom() + e
  }

  #[test]
  fn every_align_and_placement_builds_inside_its_bounds() {
    let bounds = Bounds::new(point(px(10.), px(20.)), size(px(300.), px(200.)));
    for placement in ALL {
      for align in [Align::Left, Align::Right, Align::Relative(150.)] {
        let path = panel_path(bounds, px(12.), align, placement).expect("a path");
        assert!(!path.vertices.is_empty());
        assert!(inside(&path, bounds), "{placement:?} {:?}", path.bounds);
      }
    }
  }

  #[test]
  fn degenerate_bounds_do_not_panic() {
    for placement in ALL {
      for align in [Align::Left, Align::Right, Align::Relative(0.)] {
        for (w, h, n) in [
          (0., 0., 12.),
          (300., 5., 12.),
          (5., 300., 12.),
          (300., 200., 0.),
        ] {
          let bounds = Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)));
          let _ = panel_path(bounds, px(n), align, placement);
        }
      }
    }
  }

  #[test]
  fn blockers_cover_their_display_and_take_no_keys() {
    let options = blocker_options(DisplayId::new(7));
    assert_eq!(options.display_id, Some(DisplayId::new(7)));
    let WindowKind::LayerShell(shell) = options.kind else {
      panic!("not a layer surface");
    };
    assert_eq!(shell.namespace, format!("{PANEL_NAME}_blocker"));
    assert_eq!(shell.anchor, Anchor::all());
    assert!(matches!(
      shell.keyboard_interactivity,
      KeyboardInteractivity::None
    ));
  }
}
