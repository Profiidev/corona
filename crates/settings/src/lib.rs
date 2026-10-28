//! The settings window: every setting of the [`corona_config::Config`], on
//! pages like Noctalia's. Changes apply as they are made and are kept in the
//! settings file, over the user's config files. Each crate registers its own
//! pages; the bar's widgets register their option forms.

rust_i18n::i18n!("../../assets/locales", fallback = "en");

use std::collections::BTreeMap;

use anyhow::Result;
use corona_config::bar::BarConfig;
use corona_surface::bar::Widget;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, FocusHandle, Focusable, Global,
  InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, TitlebarOptions,
  Window, WindowBounds, WindowDecorations, WindowOptions,
  assets::IconName,
  component::{
    Icon, Root,
    group_box::GroupBoxVariant,
    setting::{SelectIndex, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
  },
  px, size,
};
use rust_i18n::t;

pub use bar::{Form, Slot};

mod appearance;
pub mod bar;
pub mod commands;

const APP_NAME_SETTINGS: &str = "corona-settings";

/// A page of the settings window
pub trait SettingsPage: 'static {
  /// What `corona ipc settings open <page>` takes
  const ID: &'static str;
  /// Its place in the sidebar, lowest first
  const ORDER: i32;

  fn page(cx: &App) -> SettingPage;
}

struct PageEntry {
  id: &'static str,
  order: i32,
  page: fn(&App) -> SettingPage,
}

/// The options form of a bar widget type
#[derive(Clone, Copy)]
pub struct WidgetOptions {
  pub form: Form,
  /// The row's label when it says more than the widget's name
  pub label: Option<fn(&BarConfig, Slot) -> String>,
}

/// The settings pages, in sidebar order, and the bar widgets' option forms
#[derive(Default)]
pub struct SettingsRegistry {
  pages: Vec<PageEntry>,
  widget_options: BTreeMap<&'static str, WidgetOptions>,
  on_open: Option<fn(&mut App)>,
}

impl Global for SettingsRegistry {}

impl SettingsRegistry {
  pub fn register<P: SettingsPage>(&mut self) -> &mut Self {
    self.pages.retain(|p| p.id != P::ID);
    let at = self.pages.partition_point(|p| p.order <= P::ORDER);
    self.pages.insert(
      at,
      PageEntry {
        id: P::ID,
        order: P::ORDER,
        page: P::page,
      },
    );
    self
  }

  /// The options form of widgets of type `W`
  pub fn widget_options<W: Widget>(&mut self, options: WidgetOptions) -> &mut Self {
    self.widget_options.insert(W::NAME, options);
    self
  }

  /// Runs before the window opens, like refreshing what the plugins page lists
  pub fn on_open(&mut self, on_open: fn(&mut App)) -> &mut Self {
    self.on_open = Some(on_open);
    self
  }

  /// The page ids, in sidebar order
  pub fn ids(&self) -> Vec<&'static str> {
    self.pages.iter().map(|p| p.id).collect()
  }

  pub fn widget_options_of(&self, widget_type: &str) -> Option<WidgetOptions> {
    self.widget_options.get(widget_type).copied()
  }

  /// Where `page` is in the sidebar, the first page when it is none or unknown
  fn index(&self, page: Option<&str>) -> usize {
    page
      .and_then(|p| self.pages.iter().position(|e| e.id == p))
      .unwrap_or_default()
  }
}

pub trait SettingsExt {
  fn settings(&self) -> &SettingsRegistry;
  fn settings_mut(&mut self) -> &mut SettingsRegistry;
  /// Shows the settings, on `P` when it opens
  fn open_settings<P: SettingsPage>(&mut self) -> Result<()>;
}

impl SettingsExt for App {
  fn settings(&self) -> &SettingsRegistry {
    static EMPTY: SettingsRegistry = SettingsRegistry {
      pages: Vec::new(),
      widget_options: BTreeMap::new(),
      on_open: None,
    };
    self.try_global::<SettingsRegistry>().unwrap_or(&EMPTY)
  }

  fn settings_mut(&mut self) -> &mut SettingsRegistry {
    self.default_global::<SettingsRegistry>()
  }

  fn open_settings<P: SettingsPage>(&mut self) -> Result<()> {
    open(Some(P::ID), self)
  }
}

/// The bar and appearance pages and the settings commands
pub fn init(cx: &mut App) {
  cx.settings_mut()
    .register::<AppearanceSettings>()
    .register::<BarSettings>();
  commands::register_commands(cx);
}

pub struct AppearanceSettings;

impl SettingsPage for AppearanceSettings {
  const ID: &'static str = "appearance";
  const ORDER: i32 = 0;

  fn page(cx: &App) -> SettingPage {
    appearance::page(cx)
  }
}

pub struct BarSettings;

impl SettingsPage for BarSettings {
  const ID: &'static str = "bar";
  const ORDER: i32 = 10;

  fn page(cx: &App) -> SettingPage {
    bar::page(cx)
  }
}

/// A page with its title, icon and groups
pub fn page(
  title: impl Into<SharedString>,
  icon: IconName,
  groups: Vec<SettingGroup>,
) -> SettingPage {
  SettingPage::new(title)
    .icon(Icon::new(icon))
    .resettable(true)
    .groups(groups)
}

pub fn group(title: impl Into<SharedString>, items: Vec<SettingItem>) -> SettingGroup {
  SettingGroup::new().title(title).items(items)
}

pub fn item(
  title: impl Into<SharedString>,
  description: impl Into<String>,
  field: SettingField<impl Clone + PartialEq + Send + Sync + 'static>,
) -> SettingItem {
  SettingItem::new(title, field).description(description.into())
}

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
    let pages: Vec<_> = cx.settings().pages.iter().map(|p| p.page).collect();
    gpui_kit::div().track_focus(&self.focus).size_full().child(
      Settings::new("corona-settings")
        .with_group_variant(GroupBoxVariant::Outline)
        .default_selected_index(SelectIndex {
          page_ix: self.page,
          group_ix: None,
        })
        .pages(pages.into_iter().map(|page| page(cx)).collect::<Vec<_>>()),
    )
  }
}

/// The settings window, while it is open
pub fn window(cx: &App) -> Option<AnyWindowHandle> {
  let handle = cx.try_global::<OpenSettings>()?.0?;
  // gone once the user closed it
  cx.windows().contains(&handle).then_some(handle)
}

/// Shows the settings, on `page` when it opens.
pub fn open(page: Option<&str>, cx: &mut App) -> Result<()> {
  if let Some(handle) = window(cx) {
    return handle.update(cx, |_, window, _| window.activate_window());
  }
  let page = cx.settings().index(page);
  if let Some(on_open) = cx.settings().on_open {
    on_open(cx);
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
  if let Some(handle) = window(cx) {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
  }
}

pub fn toggle(page: Option<&str>, cx: &mut App) -> Result<()> {
  match window(cx) {
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

  struct Late;

  impl SettingsPage for Late {
    const ID: &'static str = "late";
    const ORDER: i32 = 5;

    fn page(_: &App) -> SettingPage {
      page("Late", IconName::Clock, vec![])
    }
  }

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      corona_surface::bar::BarState::init(cx);
      init(cx);
    });
  }

  #[gpui::test]
  fn pages_follow_their_order(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      cx.settings_mut().register::<Late>();
      assert_eq!(cx.settings().ids(), ["appearance", "late", "bar"]);
      // registering again moves nothing
      cx.settings_mut().register::<Late>();
      assert_eq!(cx.settings().ids(), ["appearance", "late", "bar"]);
      let registry = cx.settings();
      assert_eq!(registry.index(None), 0);
      assert_eq!(registry.index(Some("nope")), 0);
      assert_eq!(registry.index(Some("")), 0);
      assert_eq!(registry.index(Some("bar")), 2);
    });
  }

  #[gpui::test]
  fn every_page_renders(cx: &mut TestAppContext) {
    setup(cx);
    for page in cx.update(|cx| cx.settings().ids()) {
      cx.update(|cx| open(Some(page), cx)).unwrap();
      let handle = cx.update(|cx| window(cx)).expect("settings open");
      cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
      cx.update(close);
      cx.run_until_parked();
      assert!(cx.update(|cx| window(cx)).is_none(), "{page}");
    }
  }

  #[gpui::test]
  fn toggle_opens_once(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| toggle(None, cx)).unwrap();
    let first = cx.update(|cx| window(cx)).expect("opened");
    // opening again keeps the window
    cx.update(|cx| open(Some("bar"), cx)).unwrap();
    assert_eq!(cx.update(|cx| window(cx)), Some(first));
    cx.update(|cx| toggle(None, cx)).unwrap();
    cx.run_until_parked();
    assert!(cx.update(|cx| window(cx)).is_none());
    // closing a closed window is fine
    cx.update(close);
  }
}
