use std::collections::HashMap;

use anyhow::{Context as _, Result};
use corona_config::{
  APP_NAME, ConfigProvider, bar::BarConfig, observe_section, placement::Placement,
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Axis, Bounds, DisplayId, Entity, EntityId, Global, Pixels,
  Styled, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
  WindowId, WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;

use crate::{
  bar::{BAR_NAMESPACE, Widget, base::Bar, widgets::WidgetData},
  per_display::PerDisplay,
};

pub struct BarState {
  widgets: HashMap<String, WidgetData>,
  bars: HashMap<WindowId, WeakEntity<Bar>>,
  displays: Option<Entity<PerDisplay>>,
  /// The theme radius the open bars were sized for
  opened_radius: Pixels,
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState {
      widgets: HashMap::new(),
      bars: HashMap::new(),
      displays: None,
      opened_radius: Pixels::ZERO,
    });
  }

  /// Opens the configured bars on every monitor, and opens them again whenever
  /// the `[bar]` settings change, or the theme's radius: their size, flare and
  /// widgets are fixed once open.
  pub fn spawn_bars(cx: &mut App) {
    Self::open_bars(cx);
    observe_section(cx, |c| &c.bar, |_, cx| Self::reopen(cx));
    // registered after the theme's own observer, so the theme is already applied
    observe_section(
      cx,
      |c| &c.theme,
      |_, cx| {
        if cx.theme().radius != cx.bar().opened_radius {
          Self::reopen(cx);
        }
      },
    );
  }

  /// Closes the bars and opens them again, to show widgets registered since
  pub fn reopen(cx: &mut App) {
    // not open yet: they open with the widgets there are then
    let Some(displays) = cx.bar_mut().displays.take() else {
      return;
    };
    PerDisplay::close(displays, cx);
    Self::open_bars(cx);
  }

  fn open_bars(cx: &mut App) {
    cx.bar_mut().opened_radius = cx.theme().radius;
    let displays = PerDisplay::new(cx, |cx, display_id| {
      let bars: Vec<BarConfig> = cx.config().bar.values().cloned().collect();
      bars
        .into_iter()
        .filter_map(|config| {
          Self::create(cx, config, display_id)
            .inspect_err(|e| error!("Failed to create bar: {e}"))
            .ok()
        })
        .collect()
    });
    cx.bar_mut().displays = Some(displays);
  }

  pub fn register<W: Widget>(&mut self) -> &mut Self {
    self.register_data(WidgetData::new::<W>())
  }

  /// Registers a widget built at runtime, like a plugin's
  pub fn register_data(&mut self, data: WidgetData) -> &mut Self {
    self.widgets.insert(data.name.clone(), data);
    self
  }

  pub fn unregister(&mut self, name: &str) -> &mut Self {
    self.widgets.remove(name);
    self
  }

  /// Every widget a bar can show, sorted
  pub fn widget_names(cx: &App) -> Vec<String> {
    let mut names: Vec<String> = cx.global::<BarState>().widgets.keys().cloned().collect();
    names.sort();
    names
  }

  pub fn widget(&self, name: &str) -> Option<&WidgetData> {
    self.widgets.get(name)
  }

  pub(crate) fn create(
    cx: &mut App,
    config: BarConfig,
    display_id: DisplayId,
  ) -> Result<AnyWindowHandle> {
    let flare = (cx.theme().radius * 2).as_f32();
    // Resolved here, not in the widgets: a layer-shell window has no output
    // until the compositor sends `wl_surface::enter`, so `window.display()` is
    // still `None` while the widgets are being built.
    let display_uuid = cx
      .find_display(display_id)
      .context("No display for the bar")?
      .uuid()?;

    let handle = cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: config.position.anchor(),
          exclusive_zone: Some(px(config.thickness)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Top,
          namespace: BAR_NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_decorations: Some(WindowDecorations::Client),
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: config.position.size(config.thickness + flare, 0.),
        })),
        display_id: Some(display_id),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Bar::new(config, window, cx, display_uuid));

        let state = cx.global_mut::<BarState>();
        state.bars.retain(|_, bar| bar.upgrade().is_some());
        state
          .bars
          .insert(window.window_handle().window_id(), view.downgrade());

        cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
      },
    )?;

    Ok(handle.into())
  }

  /// The window and bar showing widget `widget_id`
  pub(crate) fn bar_with_widget(
    widget_id: EntityId,
    cx: &App,
  ) -> Option<(AnyWindowHandle, Entity<Bar>)> {
    let (window_id, bar) = cx.global::<BarState>().bars.iter().find_map(|(id, bar)| {
      let bar = bar.upgrade()?;
      bar.read(cx).widget_bounds(widget_id).map(|_| (*id, bar))
    })?;
    let handle = cx
      .windows()
      .into_iter()
      .find(|h| h.window_id() == window_id)?;
    Some((handle, bar))
  }

  /// Where widget `widget_id` is shown: its bar's side and whether it goes
  /// without a pill of its own; `None` when it is in no bar
  pub fn widget_place(widget_id: EntityId, cx: &App) -> Option<(Placement, bool)> {
    let (_, bar) = Self::bar_with_widget(widget_id, cx)?;
    let bar = bar.read(cx);
    Some((bar.placement(), bar.is_bare(widget_id)))
  }

  pub fn get(window: &Window, cx: &App) -> Option<Entity<Bar>> {
    cx.global::<BarState>()
      .bars
      .get(&window.window_handle().window_id())?
      .upgrade()
  }

  pub fn placement(window: &Window, cx: &App) -> Placement {
    Self::get(window, cx).map_or(Placement::Top, |bar| bar.read(cx).placement())
  }

  /// Whether the widget goes without a pill of its own
  pub fn is_bare(window: &Window, cx: &App, widget_id: EntityId) -> bool {
    Self::get(window, cx).is_some_and(|bar| bar.read(cx).is_bare(widget_id))
  }

  pub(crate) fn bars_on(display_id: DisplayId, cx: &App) -> Vec<Entity<Bar>> {
    let state = cx.global::<BarState>();
    let Some(displays) = &state.displays else {
      return Vec::new();
    };
    displays
      .read(cx)
      .windows(display_id)
      .iter()
      .filter_map(|handle| state.bars.get(&handle.window_id())?.upgrade())
      .collect()
  }

  pub fn bar_axis(window: &Window, cx: &App) -> Axis {
    if Self::placement(window, cx).is_horizontal() {
      Axis::Horizontal
    } else {
      Axis::Vertical
    }
  }
}

pub trait BarExt {
  fn bar(&self) -> &BarState;
  fn bar_mut(&mut self) -> &mut BarState;
}

impl BarExt for App {
  fn bar(&self) -> &BarState {
    self.global::<BarState>()
  }

  fn bar_mut(&mut self) -> &mut BarState {
    self.global_mut::<BarState>()
  }
}

#[cfg(test)]
mod tests {
  use corona_config::bar::{WidgetConfig, WidgetEntry};
  use gpui_kit::TestAppContext;

  use super::*;
  use crate::test_support::{self, Label, Toggle, draw_all, plain_window, windows};

  fn entry(widget_type: &str, options: Option<serde_json::Value>) -> WidgetEntry {
    WidgetEntry {
      widget_type: widget_type.into(),
      options,
    }
  }

  fn widget(widget_type: &str) -> WidgetConfig {
    WidgetConfig::Widget(entry(widget_type, None))
  }

  fn group(types: &[&str]) -> WidgetConfig {
    WidgetConfig::Group {
      group: types.iter().map(|t| entry(t, None)).collect(),
    }
  }

  fn config(position: Placement) -> BarConfig {
    BarConfig {
      position,
      start: vec![widget("label"), widget("unknown")],
      center: vec![
        group(&["unknown"]),
        widget("toggle"),
        WidgetConfig::Widget(entry("toggle", Some(serde_json::json!(true)))),
      ],
      end: vec![group(&["label", "toggle", "unknown"])],
      ..Default::default()
    }
  }

  fn setup(cx: &mut TestAppContext) {
    test_support::setup(cx);
    cx.update(|cx| {
      cx.bar_mut().register::<Toggle>().register::<Label>();
    });
  }

  fn create(config: BarConfig, cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.update(|cx| {
      let display = cx.displays()[0].id();
      BarState::create(cx, config, display).unwrap()
    })
  }

  fn bar(handle: AnyWindowHandle, cx: &mut TestAppContext) -> Option<Entity<Bar>> {
    cx.update_window(handle, |_, window, cx| BarState::get(window, cx))
      .unwrap()
  }

  #[gpui_kit::test]
  fn widget_names_are_sorted(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      assert_eq!(BarState::widget_names(cx), ["label", "toggle"]);
      assert!(cx.bar().widget("label").is_some());
      assert!(cx.bar().widget("nope").is_none());
    });
  }

  #[gpui_kit::test]
  fn runtime_widgets_register_and_unregister(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      cx.bar_mut()
        .register_data(WidgetData::from_fn("plugin:w", |_, cx, _, _| {
          Some(cx.new(|_| Label(Default::default())).into())
        }));
      assert_eq!(BarState::widget_names(cx), ["label", "plugin:w", "toggle"]);
    });
    let handle = create(
      BarConfig {
        start: vec![widget("plugin:w")],
        ..Default::default()
      },
      cx,
    );
    let bar = bar(handle, cx).unwrap();
    bar.read_with(cx, |bar, _| assert_eq!(bar.sections()[0].len(), 1));
    cx.update(|cx| {
      cx.bar_mut().unregister("plugin:w");
      assert_eq!(BarState::widget_names(cx), ["label", "toggle"]);
    });
  }

  #[gpui_kit::test]
  fn a_window_without_a_bar_gets_defaults(cx: &mut TestAppContext) {
    setup(cx);
    let handle = plain_window(cx);
    cx.update_window(handle, |_, window, cx| {
      assert!(BarState::get(window, cx).is_none());
      assert_eq!(BarState::placement(window, cx), Placement::Top);
      assert_eq!(BarState::bar_axis(window, cx), Axis::Horizontal);
      let id = cx.new(|_| ()).entity_id();
      assert!(!BarState::is_bare(window, cx, id));
    })
    .unwrap();
    cx.update(|cx| {
      let display = cx.displays()[0].id();
      assert!(BarState::bars_on(display, cx).is_empty());
    });
  }

  #[gpui_kit::test]
  fn create_drops_unknown_widgets_and_empty_groups(cx: &mut TestAppContext) {
    setup(cx);
    let handle = create(config(Placement::Left), cx);
    let bar = bar(handle, cx).expect("registered");
    cx.update_window(handle, |_, window, cx| {
      assert_eq!(BarState::placement(window, cx), Placement::Left);
      assert_eq!(BarState::bar_axis(window, cx), Axis::Vertical);
    })
    .unwrap();
    bar.read_with(cx, |bar, _| {
      let [start, center, end] = bar.sections();
      assert_eq!(start.len(), 1);
      // the group of only unknown widgets is gone
      assert_eq!(center.len(), 2);
      assert_eq!(end.len(), 1);
      assert_eq!(end[0].len(), 2);
      for view in &end[0] {
        assert!(bar.is_bare(view.entity_id()));
      }
      assert!(!bar.is_bare(start[0][0].entity_id()));
      assert!(!bar.is_bare(center[0][0].entity_id()));
    });
  }

  #[gpui_kit::test]
  fn without_capsules_every_widget_is_bare(cx: &mut TestAppContext) {
    setup(cx);
    let handle = create(
      BarConfig {
        capsule: false,
        ..config(Placement::Top)
      },
      cx,
    );
    let bar = bar(handle, cx).unwrap();
    bar.read_with(cx, |bar, _| {
      assert!(bar.is_bare(bar.sections()[0][0][0].entity_id()))
    });
  }

  #[gpui_kit::test]
  fn bars_render_on_every_side_and_track_widgets(cx: &mut TestAppContext) {
    setup(cx);
    for placement in [
      Placement::Top,
      Placement::Bottom,
      Placement::Left,
      Placement::Right,
    ] {
      let handle = create(config(placement), cx);
      // a layer surface gets its length from the compositor
      cx.simulate_window_resize(handle, gpui_kit::size(px(800.), px(800.)));
      draw_all(cx);
      let bar = bar(handle, cx).unwrap();
      bar.read_with(cx, |bar, _| {
        assert!(bar.bounds().size.width > px(0.));
        assert!(
          bar
            .widget_bounds(bar.sections()[0][0][0].entity_id())
            .is_some()
        );
        assert!(bar.widget_bounds(EntityId::from(u64::MAX)).is_none());
      });
    }
  }

  #[gpui_kit::test]
  fn closed_bars_are_forgotten(cx: &mut TestAppContext) {
    setup(cx);
    let first = create(config(Placement::Top), cx);
    let _ = cx.update_window(first, |_, window, _| window.remove_window());
    cx.run_until_parked();
    create(config(Placement::Top), cx);
    cx.update(|cx| {
      let bars = &cx.bar().bars;
      assert_eq!(bars.len(), 1);
      assert!(!bars.contains_key(&first.window_id()));
    });
  }

  #[gpui_kit::test]
  fn spawn_bars_reopens_on_bar_settings(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(BarState::spawn_bars);
    cx.run_until_parked();
    let before = windows(cx);
    let first = cx.update(|cx| cx.bar().displays.clone().unwrap());
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.bar.clear();
      cx.set_global(config);
    });
    cx.run_until_parked();
    let second = cx.update(|cx| cx.bar().displays.clone().unwrap());
    assert_ne!(first, second);
    // a theme change that keeps the radius keeps the bars
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.theme.shadow = !config.theme.shadow;
      cx.set_global(config);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.bar().displays.clone().unwrap()), second);
    assert!(windows(cx) <= before);
  }
}
