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
