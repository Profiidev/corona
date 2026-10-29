//! The shell's settings pages and bar widget forms, registered with
//! [`corona_settings`]

use gpui_kit::App;

mod pages;
mod widget_forms;

pub fn init(cx: &mut App) {
  pages::register(cx);
  widget_forms::register(cx);
}

#[cfg(test)]
mod tests {
  use super::*;
  use corona_settings::SettingsExt;
  use gpui_kit::{self as gpui, AppContext as _, TestAppContext, test::TestWindowExt};

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      corona_settings::init(cx);
      init(cx);
      // the integrations' pages, rendered here too
      cx.settings_mut()
        .register::<corona_weather::settings::LocationSettings>()
        .register::<corona_pipewire::settings::PrivacySettings>()
        .register::<corona_notifications::settings::NotificationSettings>();
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
