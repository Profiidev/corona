use serde::Deserialize;

use crate::hypr_data_cmd;

#[derive(Debug, Deserialize)]
pub struct CursorPosition {
  pub x: i32,
  pub y: i32,
}

hypr_data_cmd!(
  cursor_position,
  "cursorpos",
  CursorPosition,
  (i32, i32),
  |c: CursorPosition| (c.x, c.y)
);

#[cfg(test)]
mod tests {
  use crate::hyprland::fake::FakeHyprland;

  #[test]
  fn cursor_position() {
    let hypr = FakeHyprland::start();
    assert_eq!(hypr.ipc().cursor_position().unwrap(), (-5, 1200));
    hypr.answer("j/cursorpos", r#"{"x": 1.5, "y": 2}"#);
    assert!(hypr.ipc().cursor_position().is_err());
  }
}
