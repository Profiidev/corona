use std::{
  collections::{HashMap, HashSet},
  time::Duration,
};

use anyhow::{Context as _, Result};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, DisplayId, Entity, Global, Styled, Subscription,
  WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowId,
  WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;
use uuid::Uuid;

use crate::{
  APP_NAME,
  bar::{BAR_NAMESPACE, base::Bar},
  compositor::{CompositorExt, event::CompositorEvent},
  config::{ConfigProvider, bar::BarConfig},
  error::ErrorLogExt,
  utils::display_uuid,
};

const DISPLAY_WAIT_TICK: Duration = Duration::from_millis(16);
const DISPLAY_WAIT_TICKS: usize = 60;

pub struct BarState {
  bars: HashMap<WindowId, WeakEntity<Bar>>,
  windows: HashMap<DisplayId, Vec<AnyWindowHandle>>,
  subscription: Option<Subscription>,
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState {
      bars: HashMap::new(),
      windows: HashMap::new(),
      subscription: None,
    });

    let emitter = cx.compositor().emitter().clone();
    let subscription = cx.subscribe(&emitter, |_, event, cx| {
      if matches!(event, CompositorEvent::Monitor(_)) {
        Self::reconcile_soon(cx);
      }
    });
    cx.global_mut::<BarState>().subscription = Some(subscription);

    Self::reconcile_soon(cx);
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
    let monitors = cx
      .compositor()
      .list_monitors()
      .log_err()
      .unwrap_or_default();

    let displays: HashSet<Uuid> = cx.displays().iter().filter_map(|d| d.uuid().ok()).collect();

    monitors
      .iter()
      .filter(|m| !m.disabled)
      .all(|m| displays.contains(&display_uuid(&m.name)))
  }

  fn reconcile(cx: &mut App) {
    let monitors = cx
      .compositor()
      .list_monitors()
      .log_err()
      .unwrap_or_default();

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

  pub fn create(cx: &mut App, config: BarConfig, display_id: DisplayId) -> Result<AnyWindowHandle> {
    let flare = cx.theme().radius_2xl().as_f32();
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

        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )?;

    Ok(handle.into())
  }

  pub fn get(window: &Window, cx: &App) -> Option<Entity<Bar>> {
    cx.global::<BarState>()
      .bars
      .get(&window.window_handle().window_id())?
      .upgrade()
  }
}
