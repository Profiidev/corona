//! One OSD type per kind the settings can turn off, each showing a shared view

use corona_surface::{osd::ToggleOsd, osd_kind};

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
osd_kind!(
  StayAwakeOsd(ToggleOsd),
  "stay_awake",
  "app.settings.osd.stay_awake.title",
  "app.settings.osd.stay_awake.description"
);
