//! One OSD type per kind the settings can turn off, each showing a shared view

use corona_surface::osd::{LevelOsd, Osd, ToggleOsd};
use gpui_kit::{App, Context, IntoElement, Pixels, Render, SharedString, Size, Window};
use rust_i18n::t;

macro_rules! kind {
  ($name:ident($view:ty), $kind:literal, $title:literal, $description:literal) => {
    pub struct $name(pub $view);

    impl From<$view> for $name {
      fn from(view: $view) -> Self {
        Self(view)
      }
    }

    impl Osd for $name {
      const KIND: &'static str = $kind;

      fn title() -> SharedString {
        t!($title).into()
      }

      fn description() -> SharedString {
        t!($description).into()
      }

      fn size(&self, _: &App) -> Size<Pixels> {
        self.0.size()
      }
    }

    impl Render for $name {
      fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.view(cx)
      }
    }
  };
}

kind!(
  VolumeOsd(LevelOsd),
  "volume",
  "app.settings.osd.volume.title",
  "app.settings.osd.volume.description"
);
kind!(
  BrightnessOsd(LevelOsd),
  "brightness",
  "app.settings.osd.brightness.title",
  "app.settings.osd.brightness.description"
);
kind!(
  WifiOsd(ToggleOsd),
  "wifi",
  "app.settings.osd.wi_fi.title",
  "app.settings.osd.wi_fi.description"
);
kind!(
  BluetoothOsd(ToggleOsd),
  "bluetooth",
  "app.settings.osd.bluetooth.title",
  "app.settings.osd.bluetooth.description"
);
kind!(
  DndOsd(ToggleOsd),
  "dnd",
  "app.settings.osd.do_not_disturb.title",
  "app.settings.osd.do_not_disturb.description"
);
kind!(
  PowerProfileOsd(ToggleOsd),
  "power_profile",
  "app.settings.osd.power_profile.title",
  "app.settings.osd.power_profile.description"
);
kind!(
  PrivacyOsd(ToggleOsd),
  "privacy",
  "app.settings.osd.privacy.title",
  "app.settings.osd.privacy.description"
);
kind!(
  KeyboardLayoutOsd(ToggleOsd),
  "keyboard_layout",
  "app.settings.osd.keyboard_layout.title",
  "app.settings.osd.keyboard_layout.description"
);
kind!(
  LockKeysOsd(ToggleOsd),
  "lock_keys",
  "app.settings.osd.lock_keys.title",
  "app.settings.osd.lock_keys.description"
);
kind!(
  StayAwakeOsd(ToggleOsd),
  "stay_awake",
  "app.settings.osd.stay_awake.title",
  "app.settings.osd.stay_awake.description"
);
