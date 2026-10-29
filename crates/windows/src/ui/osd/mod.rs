//! The keyboard layout and lock key OSDs

use corona_surface::{
  osd::{Osd, OsdExt, ToggleOsd},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

mod keyboard;
pub mod lock_keys;

osd_kind!(
  KeyboardLayoutOsd(ToggleOsd),
  "keyboard_layout",
  "app.settings.osd.keyboard_layout.title",
  "app.settings.osd.keyboard_layout.description"
);
osd_kind!(
  LockKeysOsd(ToggleOsd),
  "lock_keys",
  "app.settings.osd.lock_keys.title",
  "app.settings.osd.lock_keys.description"
);

/// Shows `osd` as the kind `T`, logging what went wrong
fn show<T: Osd>(osd: impl Into<T>, cx: &mut App) {
  let _ = cx.show_osd(osd.into()).log_err();
}

pub(crate) fn init(cx: &mut App) {
  cx.osd_mut()
    .register::<KeyboardLayoutOsd>()
    .register::<LockKeysOsd>();
  keyboard::init(cx);
}
