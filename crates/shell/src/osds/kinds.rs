//! One OSD type per kind the settings can turn off, each showing a shared view

use corona_surface::{osd::ToggleOsd, osd_kind};

osd_kind!(
  StayAwakeOsd(ToggleOsd),
  "stay_awake",
  "app.settings.osd.stay_awake.title",
  "app.settings.osd.stay_awake.description"
);
