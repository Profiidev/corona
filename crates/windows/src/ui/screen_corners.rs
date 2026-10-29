//! Rounded screen corners where no bar rounds them: every bar flares into the
//! two corners at its inner edge, the others get a small layer of their own.

use std::collections::HashSet;

use anyhow::Result;
use corona_config::{APP_NAME, ConfigProvider, observe_section};
use corona_surface::per_display::PerDisplay;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Entity, Global, IntoElement,
  PathBuilder, Pixels, Render, Size, Styled, Window, WindowBackgroundAppearance, WindowBounds,
  WindowDecorations, WindowKind, WindowOptions,
  base::Root,
  canvas,
  component::ActiveTheme,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;

const NAMESPACE: &str = "corona_screen_corner";

const CORNERS: [Anchor; 4] = [
  Anchor::TOP.union(Anchor::LEFT),
  Anchor::TOP.union(Anchor::RIGHT),
  Anchor::BOTTOM.union(Anchor::LEFT),
  Anchor::BOTTOM.union(Anchor::RIGHT),
];

struct ScreenCorners {
  displays: Option<Entity<PerDisplay>>,
}

impl Global for ScreenCorners {}

pub fn init(cx: &mut App) {
  cx.set_global(ScreenCorners { displays: None });
  open(cx);
  // the radius follows the theme, which corners are free the bars
  observe_section(cx, |c| &c.theme, |_, cx| reopen(cx));
  observe_section(cx, |c| &c.bar, |_, cx| reopen(cx));
}

fn reopen(cx: &mut App) {
  if let Some(displays) = cx.global_mut::<ScreenCorners>().displays.take() {
    PerDisplay::close(displays, cx);
  }
  open(cx);
}

fn open(cx: &mut App) {
  if !cx.config().theme.screen_corners {
    return;
  }
  let placements: HashSet<_> = cx.config().bar.values().map(|b| b.position).collect();
  let corners = free_corners(placements.iter().map(|p| p.anchor()));
  // the bars flare by the same radius
  let radius = cx.theme().radius * 2;
  let displays = PerDisplay::new(cx, move |cx, display| {
    corners
      .iter()
      .filter_map(|corner| {
        create(*corner, radius, display, cx)
          .inspect_err(|e| error!("failed to create screen corner: {e:#}"))
          .ok()
      })
      .collect()
  });
  cx.global_mut::<ScreenCorners>().displays = Some(displays);
}

/// The corners no bar covers; a bar's anchor holds the two corners it flares into
fn free_corners(bars: impl Iterator<Item = Anchor> + Clone) -> Vec<Anchor> {
  CORNERS
    .into_iter()
    .filter(|corner| !bars.clone().any(|bar| bar.contains(*corner)))
    .collect()
}

fn create(
  corner: Anchor,
  radius: Pixels,
  display: DisplayId,
  cx: &mut App,
) -> Result<AnyWindowHandle> {
  let handle = cx.open_window(
    WindowOptions {
      kind: WindowKind::LayerShell(LayerShellOptions {
        anchor: corner,
        // 0: stays clear of other surfaces' exclusive zones, like a bar's
        exclusive_zone: Some(px(0.)),
        exclusive_edge: None,
        margin: None,
        layer: Layer::Top,
        namespace: NAMESPACE.to_string(),
        keyboard_interactivity: KeyboardInteractivity::None,
      }),
      window_background: WindowBackgroundAppearance::Transparent,
      window_decorations: Some(WindowDecorations::Client),
      app_id: Some(APP_NAME.to_string()),
      display_id: Some(display),
      titlebar: None,
      window_bounds: Some(WindowBounds::Windowed(Bounds {
        origin: point(px(0.), px(0.)),
        size: Size::new(radius, radius),
      })),
      ..Default::default()
    },
    |window, cx| {
      // clicks go through to what is below
      window.set_input_region(Some(&[]));
      let view = cx.new(|_| Corner { corner });
      cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
    },
  )?;
  Ok(handle.into())
}

struct Corner {
  corner: Anchor,
}

impl Render for Corner {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let color = cx.theme().tokens.background;
    let corner = self.corner;
    canvas(
      |_, _, _| (),
      move |bounds, _, window, _| {
        let r = bounds.size.width;
        // drawn as the top right corner, mirrored into the others
        let (flip_x, flip_y) = (
          corner.contains(Anchor::LEFT),
          corner.contains(Anchor::BOTTOM),
        );
        let at = |x: Pixels, y: Pixels| {
          let x = if flip_x { r - x } else { x };
          let y = if flip_y { r - y } else { y };
          bounds.origin + point(x, y)
        };
        let z = px(0.);
        let mut path = PathBuilder::fill();
        path.move_to(at(z, z));
        path.line_to(at(r, z));
        path.line_to(at(r, r));
        // a mirror reverses the direction the arc sweeps
        path.arc_to(point(r, r), z, false, flip_x != flip_y, at(z, z));
        path.close();
        if let Ok(path) = path.build() {
          window.paint_path(path, color);
        }
      },
    )
    .size_full()
  }
}

#[cfg(test)]
mod tests {
  use corona_config::placement::Placement;
  use gpui_kit::layer_shell::Anchor;

  use super::free_corners;

  fn free(bars: &[Placement]) -> Vec<Anchor> {
    free_corners(bars.iter().map(Placement::anchor))
  }

  #[test]
  fn bars_cover_their_corners() {
    assert_eq!(free(&[]).len(), 4);
    assert_eq!(
      free(&[Placement::Top]),
      [
        Anchor::BOTTOM | Anchor::LEFT,
        Anchor::BOTTOM | Anchor::RIGHT
      ]
    );
    assert_eq!(
      free(&[Placement::Top, Placement::Left]),
      [Anchor::BOTTOM | Anchor::RIGHT]
    );
    assert!(free(&[Placement::Top, Placement::Bottom]).is_empty());
  }
}
