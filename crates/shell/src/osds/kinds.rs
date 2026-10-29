//! One OSD type per kind the settings can turn off, each showing a shared view

use corona_surface::{
  osd::{LevelOsd, ToggleOsd},
  osd_kind,
};

osd_kind!(
  BrightnessOsd(LevelOsd),
  "brightness",
  "app.settings.osd.brightness.title",
  "app.settings.osd.brightness.description"
);
osd_kind!(
  WifiOsd(ToggleOsd),
  "wifi",
  "app.settings.osd.wi_fi.title",
  "app.settings.osd.wi_fi.description"
);
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
