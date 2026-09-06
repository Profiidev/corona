use gpui_kit::{
  AnyView, AnyWindowHandle, AppContext, Bounds, Context, InteractiveElement, MouseButton,
  ParentElement, Path, PathBuilder, Pixels, Render, Styled, Task, canvas, component::ActiveTheme,
  div, point, prelude::FluentBuilder, px,
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
  height_anim: Anim,
  // True when opening or open, false when closing. Destroyed when closed.
  open: bool,
  window: AnyWindowHandle,
  close: Option<Task<()>>,
}

impl BasePanel {
  pub fn new<P: Panel>(
    window: AnyWindowHandle,
    panel: P,
    align: Align,
    cx: &mut Context<'_, BasePanel>,
  ) -> Self {
    let config = cx.config();
    let speed = config.animation_speed.to_duration();
    let panel = cx.new(|_| panel);

    Self {
      panel: panel.into(),
      width: P::WIDTH,
      height: P::HEIGHT,
      height_anim: Anim::new(0., speed),
      open: true,
      window,
      close: None,
      align,
    }
  }

  pub fn close(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = false;

    let speed = self.height_anim.speed();
    let window = self.window;
    self.close = Some(cx.spawn(async move |_, cx| {
      cx.background_executor().timer(speed).await;
      let _ = window.update(cx, |_, window, _| window.remove_window());
    }));

    cx.notify();
  }

  pub fn open(&mut self, cx: &mut Context<'_, BasePanel>) {
    self.open = true;
    self.close = None;
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

    self
      .height_anim
      .retarget(if self.open { self.height + bn } else { 0. });

    let (h, animating) = self.height_anim.value();
    if animating {
      window.request_animation_frame();
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
              .bottom_0()
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

  // arcs attached to bar are squashed if the panel is too short, so that they don't overlap.
  let squash = ((h - bnf) / (nf * 2.)).clamp(0., 1.);
  let ny = px(nf * squash);
  let bn = px(bnf);

  let mut p = PathBuilder::fill();
  p.move_to(point(l, t));
  p.line_to(point(r, t));

  if !matches!(align, Align::Right) {
    p.arc_to(point(n, ny), px(0.), false, false, point(r - n, t + ny));
    p.line_to(point(r - n, b - ny - bn));
    p.arc_to(point(n, ny), px(0.), false, true, point(r - n - n, b - bn));
  } else {
    p.line_to(point(r, b));
    p.arc_to(point(n, ny), px(0.), false, false, point(r - n, b - bn));
  }

  if !matches!(align, Align::Left) {
    p.line_to(point(l + n + n, b - bn));
    p.arc_to(point(n, ny), px(0.), false, true, point(l + n, b - ny - bn));
    p.line_to(point(l + n, t + ny));
    p.arc_to(point(n, ny), px(0.), false, false, point(l, t));
  } else {
    p.line_to(point(l + n, b - bn));
    p.arc_to(point(n, ny), px(0.), false, false, point(l, b));
    p.line_to(point(l, t));
  }

  p.close();
  p.build().ok()
}
