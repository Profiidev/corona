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
