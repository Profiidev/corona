use std::{
  collections::{HashMap, HashSet},
  time::Duration,
};

use anyhow::{Context as _, Result};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider, bar::BarConfig, placement::Placement};
use corona_utils::display::display_uuid;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Axis, Bounds, DisplayId, Entity, EntityId, Global, Styled,
  Subscription, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
  WindowId, WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;
use uuid::Uuid;

use crate::bar::{BAR_NAMESPACE, Widget, base::Bar, widgets::WidgetData};

const DISPLAY_WAIT_TICK: Duration = Duration::from_millis(16);
const DISPLAY_WAIT_TICKS: usize = 60;

pub struct BarState {
  widgets: HashMap<String, WidgetData>,
  bars: HashMap<WindowId, WeakEntity<Bar>>,
  windows: HashMap<DisplayId, Vec<AnyWindowHandle>>,
  display_mapping: HashMap<Uuid, DisplayId>,
  subscription: Option<Subscription>,
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState {
      widgets: HashMap::new(),
      bars: HashMap::new(),
      windows: HashMap::new(),
      display_mapping: HashMap::new(),
      subscription: None,
    });

    let monitors = cx.compositor().monitors.clone();
    let subscription = cx.observe(&monitors, |_, cx| {
      Self::reconcile_soon(cx);
    });
    cx.global_mut::<BarState>().subscription = Some(subscription);
  }

  pub fn spawn_bars(cx: &mut App) {
    Self::reconcile_soon(cx);
  }

  pub fn register<W: Widget>(&mut self) -> &mut Self {
    let data = WidgetData::new::<W>();
    self.widgets.insert(data.name.clone(), data);
    self
  }

  pub(crate) fn widget(&self, name: &str) -> Option<&WidgetData> {
    self.widgets.get(name)
  }

  /// schedules a reconciliation of the internal display list with the compositor's monitor list
  /// when the internal state has caught up with the compositor's state
  fn reconcile_soon(cx: &mut App) {
    cx.spawn(async move |cx| {
      for _ in 0..DISPLAY_WAIT_TICKS {
        if cx.update(Self::displays_ready) {
          break;
        }

        cx.background_executor().timer(DISPLAY_WAIT_TICK).await;
      }

      cx.update(Self::reconcile);
    })
    .detach();
  }

  /// checks if internal gpui display list has caught up to the compositor's monitor list
  fn displays_ready(cx: &mut App) -> bool {
    let monitors = cx.compositor().list_monitors(cx);
    let displays: HashSet<Uuid> = cx.displays().iter().filter_map(|d| d.uuid().ok()).collect();

    monitors
      .iter()
      .filter(|m| !m.disabled)
      .all(|m| displays.contains(&display_uuid(&m.name)))
  }

  fn reconcile(cx: &mut App) {
    let monitors = cx.compositor().list_monitors(cx);

    let displays: HashMap<Uuid, DisplayId> = cx
      .displays()
      .iter()
      .filter_map(|d| Some((d.uuid().ok()?, d.id())))
      .collect();

    let wanted: HashSet<DisplayId> = monitors
      .iter()
      .filter(|m| !m.disabled)
      .filter_map(|m| displays.get(&display_uuid(&m.name)).copied())
      .collect();

    cx.bar_mut().display_mapping = displays;
    let current: HashSet<DisplayId> = cx.global::<BarState>().windows.keys().copied().collect();

    for display_id in current.difference(&wanted) {
      let Some(windows) = cx.global_mut::<BarState>().windows.remove(display_id) else {
        continue;
      };

      for handle in windows {
        cx.global_mut::<BarState>().bars.remove(&handle.window_id());
        let _ = handle.update(cx, |_, window, _| window.remove_window());
      }
    }

    for display_id in wanted.difference(&current) {
      for config in cx.config().bars.clone() {
        match Self::create(cx, config, *display_id) {
          Ok(handle) => cx
            .global_mut::<BarState>()
            .windows
            .entry(*display_id)
            .or_default()
            .push(handle),
          Err(e) => error!("Failed to create bar: {}", e),
        }
      }
    }
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

  pub fn is_grouped(window: &Window, cx: &App, widget_id: EntityId) -> bool {
    Self::get(window, cx).is_some_and(|bar| bar.read(cx).is_grouped(widget_id))
  }

  pub(crate) fn display_id_for(&self, monitor: &str) -> Option<DisplayId> {
    self.display_mapping.get(&display_uuid(monitor)).copied()
  }

  pub(crate) fn bars_on(display_id: DisplayId, cx: &App) -> Vec<Entity<Bar>> {
    let state = cx.global::<BarState>();
    state
      .windows
      .get(&display_id)
      .into_iter()
      .flatten()
      .filter_map(|handle| state.bars.get(&handle.window_id())?.upgrade())
      .collect()
  }

  pub fn bar_axis(window: &Window, cx: &App) -> Axis {
    let placement = Self::get(window, cx)
      .map(|bar| bar.read(cx).placement())
      .unwrap_or(Placement::Top);

    if placement.is_horizontal() {
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
