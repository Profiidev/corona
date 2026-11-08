//! The volume and privacy OSDs

use corona_surface::{
  osd::{LevelOsd, Osd, OsdExt, ToggleOsd},
  osd_kind,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

mod privacy;
mod volume;

osd_kind!(
  VolumeOsd(LevelOsd),
  "volume",
  "app.settings.osd.volume.title",
  "app.settings.osd.volume.description"
);
osd_kind!(
  PrivacyOsd(ToggleOsd),
  "privacy",
  "app.settings.osd.privacy.title",
  "app.settings.osd.privacy.description"
);

/// Shows `osd` as the kind `T`, logging what went wrong
fn show<T: Osd>(osd: impl Into<T>, cx: &mut App) {
  let _ = cx.show_osd(osd.into()).log_err();
}

pub(crate) fn init(cx: &mut App) {
  cx.osd_mut()
    .register::<VolumeOsd>()
    .register::<PrivacyOsd>();
  volume::init(cx);
  privacy::init(cx);
}
