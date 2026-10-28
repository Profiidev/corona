use corona_surface::osd::{Osd, OsdExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

use kinds::*;

mod bluetooth;
mod brightness;
mod caffeine;
mod dnd;
mod keyboard;
mod kinds;
pub(crate) mod lock_keys;
mod power_profile;
mod privacy;
mod volume;
mod wifi;

pub fn init(cx: &mut App) {
  cx.osd_mut()
    .register::<VolumeOsd>()
    .register::<BrightnessOsd>()
    .register::<WifiOsd>()
    .register::<BluetoothOsd>()
    .register::<DndOsd>()
    .register::<PowerProfileOsd>()
    .register::<PrivacyOsd>()
    .register::<KeyboardLayoutOsd>()
    .register::<LockKeysOsd>()
    .register::<StayAwakeOsd>();
  volume::init(cx);
  brightness::init(cx);
  wifi::init(cx);
  bluetooth::init(cx);
  power_profile::init(cx);
  dnd::init(cx);
  privacy::init(cx);
  keyboard::init(cx);
  caffeine::init(cx);
}

/// Shows `osd` as the kind `T`, logging what went wrong
fn show<T: Osd>(osd: impl Into<T>, cx: &mut App) {
  let _ = cx.show_osd(osd.into()).log_err();
}
