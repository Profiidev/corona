use corona_surface::osd::{Osd, OsdExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

use kinds::*;

mod caffeine;
mod keyboard;
mod kinds;
pub(crate) mod lock_keys;

pub fn init(cx: &mut App) {
  cx.osd_mut()
    .register::<KeyboardLayoutOsd>()
    .register::<LockKeysOsd>()
    .register::<StayAwakeOsd>();
  keyboard::init(cx);
  caffeine::init(cx);
}

/// Shows `osd` as the kind `T`, logging what went wrong
fn show<T: Osd>(osd: impl Into<T>, cx: &mut App) {
  let _ = cx.show_osd(osd.into()).log_err();
}
