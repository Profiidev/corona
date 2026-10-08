use corona_config::{APP_NAME, placement::Placement};
use gpui_kit::{
  AnyWindowHandle, Bounds, Pixels, Point, Size, WindowBackgroundAppearance, WindowBounds,
  WindowDecorations, WindowKind, WindowOptions, point,
  popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions},
  px,
};

pub fn popup_options(
  parent: AnyWindowHandle,
  anchor: Bounds<Pixels>,
  placement: Placement,
  size: Size<Pixels>,
  gap: Pixels,
  grab: bool,
) -> WindowOptions {
  let (popup_anchor, gravity, offset) = match placement {
    Placement::Top => (
      PopupAnchor::Bottom,
      PopupGravity::Bottom,
      point(px(0.), gap),
    ),
    Placement::Bottom => (PopupAnchor::Top, PopupGravity::Top, point(px(0.), -gap)),
    Placement::Left => (PopupAnchor::Right, PopupGravity::Right, point(gap, px(0.))),
    Placement::Right => (PopupAnchor::Left, PopupGravity::Left, point(-gap, px(0.))),
  };

  WindowOptions {
    kind: WindowKind::AnchoredPopup(PopupOptions {
      parent,
      anchor_rect: anchor,
      anchor: popup_anchor,
      gravity,
      constraint_adjustment: PopupConstraintAdjustment::SLIDE_X
        | PopupConstraintAdjustment::SLIDE_Y,
      offset,
      grab,
    }),
    window_background: WindowBackgroundAppearance::Transparent,
    window_decorations: Some(WindowDecorations::Client),
    inactive_frame_interval: None,
    app_id: Some(APP_NAME.to_string()),
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: Point::default(),
      size,
    })),
    ..Default::default()
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{AnyWindowHandle, TestAppContext, size};

  use super::*;
  use crate::test_support;

  fn parent(cx: &mut TestAppContext) -> AnyWindowHandle {
    test_support::setup(cx);
    test_support::plain_window(cx)
  }

  #[gpui_kit::test]
  fn opens_away_from_the_bar_by_the_gap(cx: &mut TestAppContext) {
    let parent = parent(cx);
    let anchor = Bounds::new(point(px(1.), px(2.)), size(px(3.), px(4.)));
    let gap = px(6.);
    for (placement, side, offset) in [
      (Placement::Top, PopupAnchor::Bottom, point(px(0.), gap)),
      (Placement::Bottom, PopupAnchor::Top, point(px(0.), -gap)),
      (Placement::Left, PopupAnchor::Right, point(gap, px(0.))),
      (Placement::Right, PopupAnchor::Left, point(-gap, px(0.))),
    ] {
      for grab in [true, false] {
        let options = popup_options(parent, anchor, placement, size(px(10.), px(20.)), gap, grab);
        let WindowKind::AnchoredPopup(popup) = options.kind else {
          panic!("not a popup");
        };
        assert_eq!(popup.anchor, side, "{placement:?}");
        assert_eq!(popup.offset, offset, "{placement:?}");
        assert_eq!(popup.anchor_rect, anchor);
        assert_eq!(popup.grab, grab);
        assert!(popup.parent == parent);
        assert!(
          popup
            .constraint_adjustment
            .contains(PopupConstraintAdjustment::SLIDE_X | PopupConstraintAdjustment::SLIDE_Y)
        );
        assert_eq!(
          options.window_bounds,
          Some(WindowBounds::Windowed(Bounds {
            origin: Point::default(),
            size: size(px(10.), px(20.)),
          }))
        );
      }
    }
  }
}
