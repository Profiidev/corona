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

/// Shows the settings, on `page` when it opens.
pub fn open(page: Option<&str>, cx: &mut App) -> Result<()> {
  if let Some(handle) = open_window(cx) {
    return handle.update(cx, |_, window, _| window.activate_window());
  }
  let page = page
    .and_then(|p| PAGES.iter().position(|name| *name == p))
    .unwrap_or_default();

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
