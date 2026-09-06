use gpui_kit::{
  AnyView, AppContext, Bounds, Context, InteractiveElement, MouseButton, ParentElement, Path,
  PathBuilder, Pixels, Render, Styled, canvas, component::ActiveTheme, div, point,
  prelude::FluentBuilder, px, size,
};

use crate::{
  config::ConfigProvider,
  panel::{align::Align, anim::Anim, style::PanelStyle, variants::Panel},
};

pub struct BasePanel {
  panel: AnyView,
  width: f32,
  height: f32,
  align: Align,
  // True when opening or open, false when closing. Destroyed when closed.
  open: bool,
  blocks_input: bool,
  anim: Anim,
}

impl BasePanel {
  pub fn new<P: Panel>(panel: P, align: Align, cx: &mut Context<'_, BasePanel>) -> Self {
    let panel = cx.new(|_| panel);

    Self {
      panel: panel.into(),
      width: P::WIDTH,
      height: P::HEIGHT,
      open: true,
      blocks_input: true,
      anim: Anim::new(0.),
      align,
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
      let w = self.width;
      let x = match self.align {
        Align::Left => 0.,
        Align::Relative(x) => x - self.width / 2.,
        Align::Right => window.viewport_size().width.as_f32() - w,
      };
      let panel = Bounds::new(point(px(x), px(0.)), size(px(w), px(h - bn)));
      window.set_input_region(Some(&[panel]));
    }

    if !self.open && progress == 0. {
      window.remove_window();
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
          .map(|d| match self.align {
            Align::Left => d.left_0(),
            Align::Relative(x) => d.left(px(x - self.width / 2.)),
            Align::Right => d.right_0(),
          })
          .w(px(self.width + br * (nl + nr)))
          .h(px(h))
          .child(
            canvas(|_, _, _| (), {
              let n = px(br);
              let align = self.align;
              move |bounds, _, window, _| {
                if let Some(path) = panel_path(bounds, n, align) {
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
              .left(px(br * nl))
              .right(px(br * nr))
              .top_0()
              .w(px(self.width))
              .h(px(h - bn))
              .overflow_hidden()
              .child(self.panel.clone()),
          ),
      )
  }
}

fn panel_path(bounds: Bounds<Pixels>, n: Pixels, align: Align) -> Option<Path<Pixels>> {
  let (l, r, b, t) = (bounds.left(), bounds.right(), bounds.bottom(), bounds.top());
  let (h, nf) = (bounds.size.height.as_f32(), n.as_f32());

  // Space left for the notch on the side wall.
  let bnf = if !matches!(align, Align::Relative(_)) {
    nf.min(h)
  } else {
    0.
  };

  let nyf = nf.min((h - bnf) / 2.);
  let (ny, ry) = (px(nyf), px((nf * nyf).sqrt()));
  let bn = px(bnf);

  let mut p = PathBuilder::fill();
  p.move_to(point(l, t));
  p.line_to(point(r, t));

  if !matches!(align, Align::Right) {
    p.arc_to(point(n, ry), px(0.), false, false, point(r - n, t + ny));
    p.line_to(point(r - n, b - ny - bn));
    p.arc_to(point(n, ry), px(0.), false, true, point(r - n - n, b - bn));
  } else {
    p.line_to(point(r, b));
    p.arc_to(point(n, n), px(0.), false, false, point(r - n, b - bn));
  }

  if !matches!(align, Align::Left) {
    p.line_to(point(l + n + n, b - bn));
    p.arc_to(point(n, ry), px(0.), false, true, point(l + n, b - ny - bn));
    p.line_to(point(l + n, t + ny));
    p.arc_to(point(n, ry), px(0.), false, false, point(l, t));
  } else {
    p.line_to(point(l + n, b - bn));
    p.arc_to(point(n, n), px(0.), false, false, point(l, b));
    p.line_to(point(l, t));
  }

  p.close();
  p.build().ok()
}
