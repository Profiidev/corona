//! Settings sections of the integrations, and a check that every settings page renders

use gpui_kit::App;

pub(crate) mod auth;
mod brightness;
pub(crate) mod capture;
mod clipboard;
pub(crate) mod idle;
mod disks;
pub(crate) mod launcher;
pub(crate) mod notifications;
pub(crate) mod pipewire;
pub(crate) mod osk;
mod printing;
mod sysinfo;
mod tray;
pub(crate) mod weather;
pub(crate) mod windows;

pub(crate) fn register_all(cx: &mut App) {
  windows::register(cx);
  sysinfo::register(cx);
  tray::register(cx);
  weather::register(cx);
  printing::register(cx);
  pipewire::register(cx);
  notifications::register(cx);
  clipboard::register(cx);
  idle::register(cx);
  disks::register(cx);
  launcher::register(cx);
  osk::register(cx);
  auth::register(cx);
  brightness::register(cx);
  capture::register(cx);
}

#[cfg(test)]
mod tests {
  use corona_settings::SettingsExt;
  use gpui_kit::{self as gpui, AppContext as _, TestAppContext, test::TestWindowExt};

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      corona_settings::init(cx);
      // the integrations' pages, rendered here too
      cx.settings_mut()
        .register::<crate::settings::weather::LocationSettings>()
        .register::<crate::settings::pipewire::PrivacySettings>()
        .register::<crate::settings::notifications::NotificationSettings>()
        .register::<crate::settings::capture::ScreenshotSettings>()
        .register::<crate::settings::osk::OskSettings>()
        .register::<crate::settings::auth::LockscreenSettings>()
        .register::<crate::settings::windows::TaskbarSettings>()
        .register::<crate::settings::windows::WindowSwitcherSettings>()
        .register::<crate::settings::idle::IdleSettings>();
    });
  }

  #[gpui::test]
  fn every_page_renders(cx: &mut TestAppContext) {
    setup(cx);
    let pages = cx.update(|cx| cx.settings().ids());
    // the launcher and corona_script register their own
    assert_eq!(pages.len(), 15, "{pages:?}");
    for page in pages {
      cx.update(|cx| corona_settings::open(Some(page), cx))
        .unwrap();
      let handle = cx
        .update(|cx| corona_settings::window(cx))
        .expect("settings open");
      cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
      cx.update(corona_settings::close);
      cx.run_until_parked();
      assert!(
        cx.update(|cx| corona_settings::window(cx)).is_none(),
        "{page}"
      );
    }
  }
}
