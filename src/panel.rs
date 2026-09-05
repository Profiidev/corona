use std::time::{Duration, Instant};

use gpui_kit::{
  App, AppContext, Bounds, MouseButton, Path, PathBuilder, Pixels, Render, Size, Window,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas,
  component::{ActiveTheme, button::Button, *},
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::*,
  px,
};

/// Radius of the concave corner that flares the panel into the bar.
const NOTCH: Pixels = px(12.);
/// Radius of the panel's own bottom corners.
const RADIUS: Pixels = px(12.);
const WIDTH: Pixels = px(200.);
const HEIGHT: Pixels = px(200.);

pub struct Panel {
  height: Anim,
}

/// The bar/panel silhouette: flush against the bar's bottom edge, concave at
/// the top corners so the two surfaces read as one, convex at the bottom.
fn panel_path(b: Bounds<Pixels>) -> Option<Path<Pixels>> {
  let (l, r, bot) = (b.left(), b.right(), b.bottom());
  let t = b.top();
  let (n, k) = (NOTCH, RADIUS);
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

impl Panel {
  pub fn create(cx: &mut App) -> Result<gpui_kit::WindowHandle<Root>, anyhow::Error> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          // Covers the whole output — including under the bar, which is what
          // an exclusive zone of -1 asks for — so a click anywhere off the
          // panel reaches this window and dismisses it.
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: "corona_panel".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        // A zero size with opposite anchors means "fill"; a real size would be
        // centered between them instead. The bar does the same for its width.
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Panel {
          height: Anim::new(0.),
        });
        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )
  }
}

impl Render for Panel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let bg = cx.theme().tokens.status_bar;

    self.height.retarget(HEIGHT.as_f32());
    let (h, animating) = self.height.value();
    if animating {
      window.request_animation_frame();
    }

    div()
      .size_full()
      .on_mouse_down(MouseButton::Left, |_, window, _| window.remove_window())
      .child(
        div()
          // Swallows clicks so they don't reach the dismiss handler above.
          .occlude()
          .absolute()
          .left_0()
          .w(WIDTH + NOTCH * 2.)
          .h(px(h))
          .child(
            canvas(
              |_, _, _| (),
              move |bounds, _, window, _| {
                if let Some(path) = panel_path(bounds) {
                  window.paint_path(path, bg);
                }
              },
            )
            .absolute()
            .inset_0(),
          )
          .child(
            div()
              .absolute()
              .left(NOTCH)
              .right(NOTCH)
              .bottom_0()
              .p_3()
              .child(Button::new("test").label("test")),
          ),
      )
  }
}

const PANEL_ANIM: Duration = Duration::from_millis(300);

struct Anim {
  from: f32,
  to: f32,
  start: Instant,
}

impl Anim {
  fn new(value: f32) -> Self {
    Self {
      from: value,
      to: value,
      start: Instant::now(),
    }
  }

  /// Current value, and whether the animation is still running.
  fn value(&self) -> (f32, bool) {
    let t = (self.start.elapsed().as_secs_f32() / PANEL_ANIM.as_secs_f32()).min(1.);
    let eased = 1. - (1. - t) * (1. - t); // ease-out-quad, as in panel.slint
    (self.from + (self.to - self.from) * eased, t < 1.)
  }

  fn retarget(&mut self, to: f32) {
    if self.to == to {
      return;
    }
    self.from = self.value().0;
    self.to = to;
    self.start = Instant::now();
  }
}
