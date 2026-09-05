use gpui_kit::{
  App, AppContext, Bounds, Path, PathBuilder, Pixels, Render, Size, Window,
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

pub struct Panel;

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
          anchor: Anchor::TOP | Anchor::LEFT,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: "corona_panel".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          // Starts at the top of the screen so it can draw over the bar, and
          // NOTCH wider on each side to make room for the flares.
          origin: point(px(0.), px(0.)),
          size: Size::new(px(200.) + NOTCH * 2., px(200.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Panel);
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
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let bg = cx.theme().tokens.status_bar;

    div()
      .relative()
      .size_full()
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
      )
  }
}
