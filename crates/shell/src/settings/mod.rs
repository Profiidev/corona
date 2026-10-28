//! The shell's settings pages and bar widget forms, registered with
//! [`corona_settings`]

use corona_settings::SettingsExt;
use gpui_kit::App;

mod pages;
mod plugins;
mod widget_forms;

pub fn init(cx: &mut App) {
  pages::register(cx);
  widget_forms::register(cx);
  // so the plugins page lists what the sources offer now
  cx.settings_mut().on_open(|cx| {
    if cx.has_global::<corona_script::PluginManager>() {
      corona_script::PluginManager::refresh_catalogs(cx);
    }
  });
}

/// Asks about plugin `id`, which waits for approval, in the settings window.
/// False when that is not open.
pub fn review_plugin(id: &str, cx: &mut App) -> bool {
  corona_settings::window(cx).is_some_and(|handle| {
    handle
      .update(cx, |_, window, cx| plugins::review(id, window, cx))
      .is_ok()
  })
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
    });
  }

  #[gpui::test]
  fn every_page_renders(cx: &mut TestAppContext) {
    setup(cx);
    let pages = cx.update(|cx| cx.settings().ids());
    // the launcher registers its own page
    assert_eq!(pages.len(), 16, "{pages:?}");
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

  #[gpui::test]
  fn plugins_page_renders_every_control(cx: &mut TestAppContext) {
    setup(cx);
    let tmp = tempfile::tempdir().unwrap();
    let paths = corona_script::plugin::paths::Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let dir = paths.local.join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("README.md"), "# Hello").unwrap();
    std::fs::write(
      dir.join("plugin.toml"),
      r#"
id = "com.p"
name = "P"
version = "1"
description = "Every setting"
[widgets.w]
view = "main.js"
[[settings]]
key = "on"
label = "On"
type = "toggle"
default = true
[[settings]]
key = "name"
label = "Name"
type = "text"
default = "x"
[[settings]]
key = "count"
label = "Count"
type = "number"
default = 1
[[settings]]
key = "opacity"
label = "Opacity"
type = "slider"
default = 0.5
min = 0
max = 1
step = 0.1
[[settings]]
key = "units"
label = "Units"
type = "select"
default = "a"
options = [{ value = "a", label = "A" }]
[[settings]]
key = "hosts"
label = "Hosts"
type = "list"
default = ["h"]
"#,
    )
    .unwrap();
    cx.update(|cx| {
      let mut config = corona_config::Config::default();
      config.plugins.enabled = vec!["com.p".into()];
      config.plugins.approved.insert(
        "com.p".into(),
        corona_config::plugins::Approval {
          source: "local".into(),
          ..Default::default()
        },
      );
      config.plugins.source.clear();
      cx.set_global(config);
      corona_script::PluginManager::init(paths, cx);
      assert!(
        cx.global::<corona_script::PluginManager>()
          .active()
          .contains_key("com.p")
      );
    });
    cx.update(|cx| corona_settings::open(Some("plugins"), cx))
      .unwrap();
    cx.run_until_parked();
    let rows = cx.update(|cx| corona_script::PluginManager::list(cx));
    assert_eq!(rows.len(), 1);
    assert!(rows[0].enabled && rows[0].running);
    let handle = cx
      .update(|cx| corona_settings::window(cx))
      .expect("settings open");
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
      .unwrap();
    cx.update(corona_settings::close);
  }
}
