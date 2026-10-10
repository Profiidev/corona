//! The settings window: every setting of the [`corona_config::Config`], on
//! pages like Noctalia's. Changes apply as they are made and are kept in the
//! settings file, over the user's config files.

use anyhow::Result;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, FocusHandle, Focusable, Global,
  InteractiveElement, IntoElement, ParentElement, Render, Styled, TitlebarOptions, Window,
  WindowBounds, WindowDecorations, WindowOptions,
  component::{
    Root,
    group_box::GroupBoxVariant,
    setting::{SelectIndex, Settings},
  },
  px, size,
};
use rust_i18n::t;

pub use pages::PAGES;

mod bar;
mod fields;
mod pages;
mod plugins;

const APP_NAME_SETTINGS: &str = "corona-settings";

#[derive(Default)]
struct OpenSettings(Option<AnyWindowHandle>);

impl Global for OpenSettings {}

struct SettingsWindow {
  focus: FocusHandle,
  page: usize,
}

impl Focusable for SettingsWindow {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for SettingsWindow {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    gpui_kit::div().track_focus(&self.focus).size_full().child(
      Settings::new("corona-settings")
        .with_group_variant(GroupBoxVariant::Outline)
        .default_selected_index(SelectIndex {
          page_ix: self.page,
          group_ix: None,
        })
        .pages(pages::all(cx)),
    )
  }
}

fn open_window(cx: &App) -> Option<AnyWindowHandle> {
  let handle = cx.try_global::<OpenSettings>()?.0?;
  // gone once the user closed it
  cx.windows().contains(&handle).then_some(handle)
}

/// Where `page` is in the sidebar, the first page when it is none or unknown
fn page_index(page: Option<&str>) -> usize {
  page
    .and_then(|p| PAGES.iter().position(|name| *name == p))
    .unwrap_or_default()
}

/// Shows the settings, on `page` when it opens.
pub fn open(page: Option<&str>, cx: &mut App) -> Result<()> {
  if let Some(handle) = open_window(cx) {
    return handle.update(cx, |_, window, _| window.activate_window());
  }
  let page = page_index(page);
  // so the plugins page lists what the sources offer now
  if cx.has_global::<corona_script::PluginManager>() {
    corona_script::PluginManager::refresh_catalogs(cx);
  }

  let options = WindowOptions {
    titlebar: Some(TitlebarOptions {
      title: Some(t!("app.settings.window_title").into()),
      ..Default::default()
    }),
    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
      None,
      size(px(1000.), px(720.)),
      cx,
    ))),
    window_min_size: Some(size(px(640.), px(480.))),
    window_decorations: Some(WindowDecorations::Client),
    app_id: Some(APP_NAME_SETTINGS.to_string()),
    ..Default::default()
  };
  let handle = cx.open_window(options, |window, cx| {
    let view = cx.new(|cx| SettingsWindow {
      focus: cx.focus_handle(),
      page,
    });
    // so tab moves through the fields from the start
    window.focus(&view.read(cx).focus.clone(), cx);
    window.activate_window();
    cx.new(|cx| Root::new(view, window, cx))
  })?;
  cx.set_global(OpenSettings(Some(handle.into())));
  Ok(())
}

pub fn close(cx: &mut App) {
  if let Some(handle) = open_window(cx) {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
  }
}

pub fn toggle(page: Option<&str>, cx: &mut App) -> Result<()> {
  match open_window(cx) {
    Some(_) => {
      close(cx);
      Ok(())
    }
    None => open(page, cx),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::{self as gpui, TestAppContext, test::TestWindowExt};

  #[test]
  fn page_indices() {
    assert_eq!(page_index(None), 0);
    assert_eq!(page_index(Some("nope")), 0);
    assert_eq!(page_index(Some("")), 0);
    for (i, name) in PAGES.iter().enumerate() {
      assert_eq!(page_index(Some(name)), i);
    }
  }

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
    });
  }

  #[gpui::test]
  fn every_page_renders(cx: &mut TestAppContext) {
    setup(cx);
    for page in PAGES {
      cx.update(|cx| open(Some(page), cx)).unwrap();
      let handle = cx.update(|cx| open_window(cx)).expect("settings open");
      cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
      cx.update(close);
      cx.run_until_parked();
      assert!(cx.update(|cx| open_window(cx)).is_none(), "{page}");
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
    cx.update(|cx| open(Some("plugins"), cx)).unwrap();
    cx.run_until_parked();
    let rows = cx.update(|cx| corona_script::PluginManager::list(cx));
    assert_eq!(rows.len(), 1);
    assert!(rows[0].enabled && rows[0].running);
    let handle = cx.update(|cx| open_window(cx)).expect("settings open");
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
      .unwrap();
    cx.update(close);
  }

  #[gpui::test]
  fn toggle_opens_once(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| toggle(None, cx)).unwrap();
    let first = cx.update(|cx| open_window(cx)).expect("opened");
    // opening again keeps the window
    cx.update(|cx| open(Some("idle"), cx)).unwrap();
    assert_eq!(cx.update(|cx| open_window(cx)), Some(first));
    cx.update(|cx| toggle(None, cx)).unwrap();
    cx.run_until_parked();
    assert!(cx.update(|cx| open_window(cx)).is_none());
    // closing a closed window is fine
    cx.update(close);
  }
}
