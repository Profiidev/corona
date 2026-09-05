use gpui_kit::{
  AnyElement, AnyWindowHandle, Bounds, Context, InteractiveElement, MouseButton, ParentElement,
  Path, PathBuilder, Pixels, Render, Styled, Task, canvas, component::ActiveTheme, div, point, px,
};

use crate::panel::anim::Anim;

pub struct BasePanel {
  width: f32,
  height: f32,
  notch: f32,
  radius: f32,
  height_anim: Anim,
  //align: Align,
  open: bool,
  children: Vec<AnyElement>,
  window: AnyWindowHandle,
  close: Option<Task<()>>,
}

pub enum Align {
  Left,
  Relative(f32),
  Right,
}

impl BasePanel {
  pub fn new(window: AnyWindowHandle) -> Self {
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
    self
      .height_anim
      .retarget(if self.open { self.height } else { 0. });

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
          .left_0()
          .w(px(self.width + self.notch * 2.))
          .h(px(h))
          .child(
            canvas(|_, _, _| (), {
              let n = px(self.notch);
              let k = px(self.radius);
              move |bounds, _, window, _| {
                if let Some(path) = panel_path(bounds, n, k) {
                  window.paint_path(path, bg);
                }
              }
            })
            .absolute()
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

fn panel_path(b: Bounds<Pixels>, n: Pixels, k: Pixels) -> Option<Path<Pixels>> {
  let (l, r, bot) = (b.left(), b.right(), b.bottom());
  let t = b.top();
  // Concave corners run counter-clockwise (sweep = false) while the outline
  // runs clockwise; convex ones run with it.
  let mut p = PathBuilder::fill();
  p.move_to(point(l, t));
  p.line_to(point(r, t));
  p.arc_to(point(n, n), px(0.), false, false, point(r - n, t + n));
  p.line_to(point(r - n, bot - k));
  p.arc_to(point(k, k), px(0.), false, true, point(r - n - k, bot));
  p.line_to(point(l + n + k, bot));
  p.arc_to(point(k, k), px(0.), false, true, point(l + n, bot - k));
  p.line_to(point(l + n, t + n));
  p.arc_to(point(n, n), px(0.), false, false, point(l, t));
  p.close();
  p.build().ok()
}
