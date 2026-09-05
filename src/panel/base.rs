use gpui_kit::{
  AnyElement, AnyWindowHandle, Bounds, Context, InteractiveElement, MouseButton, ParentElement,
  Path, PathBuilder, Pixels, Render, Styled, Task, canvas, component::ActiveTheme, div, point,
  prelude::FluentBuilder, px,
};

use crate::panel::anim::Anim;

pub struct BasePanel {
  width: f32,
  height: f32,
  notch: f32,
  radius: f32,
  height_anim: Anim,
  align: Align,
  open: bool,
  children: Vec<AnyElement>,
  window: AnyWindowHandle,
  close: Option<Task<()>>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Align {
  Left,
  Relative(f32),
  Right,
}

impl BasePanel {
  pub fn new(
    window: AnyWindowHandle,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
  ) -> Self {
    let button_center = button_bounds.center().x.as_f32();
    let total_width = bar_bounds.size.width.as_f32();
    let half_width = 100. + 12.;

    let align = if button_center < half_width {
      Align::Left
    } else if button_center > total_width - half_width {
      Align::Right
    } else {
      Align::Relative(button_center)
    };

    Self {
      width: 200.,
      height: 200.,
      notch: 12.,
      radius: 12.,
      height_anim: Anim::new(0., std::time::Duration::from_millis(300)),
      open: true,
      children: Vec::new(),
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
}

impl Render for BasePanel {
  fn render(
    &mut self,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::prelude::Context<Self>,
  ) -> impl gpui_kit::prelude::IntoElement {
    let bg = cx.theme().tokens.status_bar;
    let bn = if !matches!(self.align, Align::Relative(_)) {
      self.notch
    } else {
      0.
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
          .w(px(
            self.width
              + self.notch
                * if let Align::Relative(_) = self.align {
                  2.
                } else {
                  1.
                },
          ))
          .h(px(h))
          .child(
            canvas(|_, _, _| (), {
              let n = px(self.notch);
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
              .left(px(self.notch))
              .right(px(self.notch))
              .bottom_0()
              .children(std::mem::take(&mut self.children)),
          ),
      )
  }
}

impl ParentElement for BasePanel {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
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
