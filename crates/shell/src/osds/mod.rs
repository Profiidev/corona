use corona_surface::osd::{Osd, OsdExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

use kinds::*;

mod brightness;
mod caffeine;
mod keyboard;
mod kinds;
pub(crate) mod lock_keys;
mod power_profile;
mod wifi;

pub fn init(cx: &mut App) {
  cx.osd_mut()
    .register::<BrightnessOsd>()
    .register::<WifiOsd>()
    .register::<PowerProfileOsd>()
    .register::<KeyboardLayoutOsd>()
    .register::<LockKeysOsd>()
    .register::<StayAwakeOsd>();
  brightness::init(cx);
  wifi::init(cx);
  power_profile::init(cx);
  keyboard::init(cx);
  caffeine::init(cx);
}

/// Shows `osd` as the kind `T`, logging what went wrong
fn show<T: Osd>(osd: impl Into<T>, cx: &mut App) {
  let _ = cx.show_osd(osd.into()).log_err();
}
