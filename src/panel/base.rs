use gpui_kit::{
  AnyView, AppContext, Bounds, Context, InteractiveElement, MouseButton, ParentElement, Path,
  PathBuilder, Pixels, Render, Styled, canvas, component::ActiveTheme, div, prelude::FluentBuilder,
  px,
};

use crate::{
  config::ConfigProvider,
  config::placement::{Placement, PlacementStyle, PlacmentBounds},
  panel::{align::Align, anim::Anim, style::PanelStyle, variants::Panel},
};

pub struct BasePanel {
  panel: AnyView,
  width: f32,
  height: f32,
  align: Align,
  placement: Placement,
  // True when opening or open, false when closing. Destroyed when closed.
  open: bool,
  blocks_input: bool,
  removing: bool,
  anim: Anim,
}

impl BasePanel {
  pub fn new<P: Panel>(
    panel: P,
    align: Align,
    placement: Placement,
    cx: &mut Context<'_, BasePanel>,
  ) -> Self {
    let panel = cx.new(|_| panel);

    Self {
      panel: panel.into(),
      width: P::WIDTH,
      height: P::HEIGHT,
      open: true,
      blocks_input: true,
      removing: false,
      anim: Anim::new(0.),
      align,
      placement,
    }
  }

  pub fn close(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = false;
    cx.notify();
  }

  pub fn open(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = true;
    cx.notify();
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
    let bg = theme.tokens.status_bar;
    let br = theme.panel_radius();

    let (bn, nl, nr) = if self.align == Align::Left {
      (br, 0., 1.)
    } else if self.align == Align::Right {
      (br, 1., 0.)
    } else {
      (0., 1., 1.)
    };

    let speed = if cx.reduce_motion() {
      std::time::Duration::ZERO
    } else {
      cx.config().animation_speed.to_duration()
    };
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
            canvas(|_, _, _| (), {
              let n = px(br);
              let align = self.align;
              let placement = self.placement;
              move |bounds, _, window, _| {
                if let Some(path) = panel_path(bounds, n, align, placement) {
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
              .anchor_p(self.placement)
              .along_p(self.placement, px(br * nl))
              .size_p(self.placement, px(self.width), px(h - bn))
              .overflow_hidden()
              .child(self.panel.clone()),
          ),
      )
  }
}

fn panel_path(
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
