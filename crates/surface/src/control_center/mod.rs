//! The control center: a sidebar of pages, each also openable on its own as a
//! panel. Pages register from their crates; `control_center.sidebar` orders them.

mod button;
mod layout;
mod nav;
mod panel;

use corona_config::{ConfigProvider, ControlCenterConfig};
use gpui_kit::{
  AnyElement, AnyView, App, AppContext, Context, Entity, Global, Render, SharedString, Window,
  assets::IconName,
};

pub use button::ControlCenterButton;
pub use layout::ControlCenterLayout;
pub use nav::ControlCenterNav;
pub use panel::{ControlCenter, Standalone};

use crate::panel::{AppPanelExt, Panel};

/// The page always first in the sidebar, if one registered it
pub const DASHBOARD: &str = "dashboard";

pub trait ControlCenterPage: Render {
  /// The key in `control_center.sidebar`, and the name of its standalone panel
  const ID: &'static str;
  const HEIGHT: f32 = <ControlCenter as Panel>::HEIGHT;

  fn icon() -> IconName;
  fn title() -> SharedString;
  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self;

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![]
  }
}

/// A shown page, its type erased
pub trait ControlCenterPageHandle {
  fn view(&self) -> AnyView;
  fn buttons(&self, cx: &mut App) -> Vec<AnyElement>;
}

impl<T: ControlCenterPage> ControlCenterPageHandle for Entity<T> {
  fn view(&self) -> AnyView {
    self.clone().into()
  }

  fn buttons(&self, cx: &mut App) -> Vec<AnyElement> {
    self.update(cx, |page, cx| page.buttons(cx))
  }
}

/// A registered page
pub struct PageEntry {
  pub id: &'static str,
  pub icon: fn() -> IconName,
  pub title: fn() -> SharedString,
  open: fn(&mut Window, &mut App) -> Box<dyn ControlCenterPageHandle>,
}

impl PageEntry {
  pub fn open(&self, window: &mut Window, cx: &mut App) -> Box<dyn ControlCenterPageHandle> {
    (self.open)(window, cx)
  }
}

/// The control center pages, in the order they registered
#[derive(Default)]
pub struct ControlCenterRegistry {
  pages: Vec<PageEntry>,
}

impl Global for ControlCenterRegistry {}

impl ControlCenterRegistry {
  pub fn pages(&self) -> &[PageEntry] {
    &self.pages
  }

  pub fn get(&self, id: &str) -> Option<&PageEntry> {
    self.pages.iter().find(|p| p.id == id)
  }

  /// A page the sidebar config may name, the dashboard is always there
  fn sidebar_page(&self, id: &str) -> Option<&'static str> {
    self.get(id).map(|p| p.id).filter(|id| *id != DASHBOARD)
  }

  /// The sidebar's pages: the dashboard, then the configured ones
  pub fn sidebar(&self, config: &ControlCenterConfig) -> Vec<&PageEntry> {
    let (ids, _) = resolve(&config.sidebar, |id| self.sidebar_page(id));
    self
      .get(DASHBOARD)
      .into_iter()
      .chain(ids.into_iter().filter_map(|id| self.get(id)))
      .collect()
  }
}

/// Registers pages, each also as its own [`Standalone`] panel
pub struct Pages<'a>(&'a mut App);

impl Pages<'_> {
  pub fn register<P: ControlCenterPage>(&mut self) -> &mut Self {
    debug_assert_ne!(P::ID, ControlCenter::NAME, "pages are panels beside it");
    self.0.panel().register::<Standalone<P>>();
    let registry = self.0.default_global::<ControlCenterRegistry>();
    registry.pages.retain(|p| p.id != P::ID);
    registry.pages.push(PageEntry {
      id: P::ID,
      icon: P::icon,
      title: P::title,
      open: |window, cx| Box::new(cx.new(|cx| P::init(window, cx))),
    });
    self
  }
}

pub trait ControlCenterExt {
  fn control_center(&self) -> &ControlCenterRegistry;
  fn control_center_mut(&mut self) -> Pages<'_>;
  /// Shows `P`: in the open control center, else as its own panel
  fn open_control_center<P: ControlCenterPage>(&mut self, window: &mut Window);
}

impl ControlCenterExt for App {
  fn control_center(&self) -> &ControlCenterRegistry {
    static EMPTY: ControlCenterRegistry = ControlCenterRegistry { pages: Vec::new() };
    self.try_global::<ControlCenterRegistry>().unwrap_or(&EMPTY)
  }

  fn control_center_mut(&mut self) -> Pages<'_> {
    Pages(self)
  }

  fn open_control_center<P: ControlCenterPage>(&mut self, window: &mut Window) {
    ControlCenter::navigate::<P>(window, self)
  }
}

/// The items `ids` name, each once, and the ids that name none or repeat one
pub fn resolve<T: PartialEq>(
  ids: &[String],
  parse: impl Fn(&str) -> Option<T>,
) -> (Vec<T>, Vec<&str>) {
  let (mut items, mut bad) = (Vec::new(), Vec::new());
  for id in ids {
    match parse(id) {
      Some(item) if !items.contains(&item) => items.push(item),
      _ => bad.push(id.as_str()),
    }
  }
  (items, bad)
}

/// Logs the sidebar ids that are skipped, now and whenever they change
pub fn warn_bad_ids(cx: &mut App) {
  fn warn(config: &ControlCenterConfig, cx: &App) {
    let registry = cx.control_center();
    let (_, sidebar) = resolve(&config.sidebar, |id| registry.sidebar_page(id));
    if !sidebar.is_empty() {
      tracing::warn!("control_center.sidebar: skipping unknown or repeated {sidebar:?}");
    }
  }
  warn(&cx.config().control_center, cx);
  corona_config::observe_section(cx, |c| &c.control_center, |config, cx| warn(config, cx));
}

#[cfg(test)]
pub(crate) mod test_pages {
  use gpui_kit::{App, Context, IntoElement, Render, SharedString, Window, assets::IconName, div};

  use super::{ControlCenterExt, ControlCenterPage};
  use crate::panel::PanelState;

  macro_rules! page {
    ($name:ident, $id:literal, $icon:ident) => {
      pub struct $name;

      impl Render for $name {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
          div()
        }
      }

      impl ControlCenterPage for $name {
        const ID: &'static str = $id;

        fn icon() -> IconName {
          IconName::$icon
        }

        fn title() -> SharedString {
          stringify!($name).into()
        }

        fn init(_: &mut Window, _: &mut Context<Self>) -> Self {
          $name
        }
      }
    };
  }

  page!(Dashboard, "dashboard", LayoutDashboard);
  page!(Audio, "audio", Volume2);
  page!(Weather, "weather", CloudSun);
  page!(Media, "media", Music);

  /// The panel registry with the test pages, the dashboard registered last
  pub fn register(cx: &mut App) {
    PanelState::init(cx);
    cx.control_center_mut()
      .register::<Audio>()
      .register::<Weather>()
      .register::<Media>()
      .register::<Dashboard>();
  }
}

#[cfg(test)]
mod tests {
  use corona_config::ControlCenterConfig;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::panel::PanelState;

  fn ids(pages: Vec<&PageEntry>) -> Vec<&'static str> {
    pages.into_iter().map(|p| p.id).collect()
  }

  #[gpui::test]
  fn sidebar_follows_config(cx: &mut TestAppContext) {
    cx.update(|cx| {
      test_pages::register(cx);
      let registry = cx.control_center();
      let all = ControlCenterConfig {
        sidebar: ["audio", "weather", "media"].map(String::from).to_vec(),
        ..Default::default()
      };
      // the dashboard comes first wherever it registered
      assert_eq!(
        ids(registry.sidebar(&all)),
        ["dashboard", "audio", "weather", "media"]
      );
      let config = ControlCenterConfig {
        sidebar: ["media", "dashboard", "nope", "audio", "media"]
          .map(String::from)
          .to_vec(),
        ..Default::default()
      };
      assert_eq!(
        ids(registry.sidebar(&config)),
        ["dashboard", "media", "audio"]
      );
    });
  }

  #[gpui::test]
  fn pages_are_panels_too(cx: &mut TestAppContext) {
    cx.update(|cx| {
      test_pages::register(cx);
      let names = PanelState::names(cx);
      for id in ["dashboard", "audio", "weather", "media"] {
        assert!(names.iter().any(|n| n == id), "{id}");
      }
      // registering again replaces the page
      cx.control_center_mut().register::<test_pages::Audio>();
      assert_eq!(cx.control_center().pages().len(), 4);
    });
  }

  #[test]
  fn resolve_skips_unknown_and_repeated() {
    let ids = ["a", "x", "b", "a"].map(String::from);
    let (found, bad) = resolve(&ids, |id| (id != "x").then(|| id.to_string()));
    assert_eq!(found, ["a", "b"]);
    assert_eq!(bad, ["x", "a"]);
  }
}
