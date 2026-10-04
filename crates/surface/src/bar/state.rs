use std::collections::HashMap;

use anyhow::{Context as _, Result};
use corona_config::{APP_NAME, ConfigProvider, bar::BarConfig, placement::Placement};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Axis, Bounds, DisplayId, Entity, EntityId, Global, Styled,
  WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowId,
  WindowKind, WindowOptions,
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
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState {
      widgets: HashMap::new(),
      bars: HashMap::new(),
      displays: None,
    });
  }

  pub fn spawn_bars(cx: &mut App) {
    let displays = PerDisplay::new(cx, |cx, display_id| {
      cx.config()
        .bars
        .clone()
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
    let data = WidgetData::new::<W>();
    self.widgets.insert(data.name.clone(), data);
    self
  }

  pub(crate) fn widget(&self, name: &str) -> Option<&WidgetData> {
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
          anchor: config.placement.anchor(),
          exclusive_zone: Some(px(config.height)),
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
          size: config.placement.size(config.height + flare, 0.),
        })),
        display_id: Some(display_id),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Bar::new(config, cx, display_uuid));

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

  pub(crate) fn get(window: &Window, cx: &App) -> Option<Entity<Bar>> {
    cx.global::<BarState>()
      .bars
      .get(&window.window_handle().window_id())?
      .upgrade()
  }

  pub fn placement(window: &Window, cx: &App) -> Placement {
    Self::get(window, cx).map_or(Placement::Top, |bar| bar.read(cx).placement())
  }

  pub fn is_grouped(window: &Window, cx: &App, widget_id: EntityId) -> bool {
    Self::get(window, cx).is_some_and(|bar| bar.read(cx).is_grouped(widget_id))
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
