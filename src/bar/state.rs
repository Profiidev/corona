use std::collections::HashMap;

use anyhow::Result;
use gpui_kit::{
  App, AppContext, Bounds, Entity, Global, Styled, WeakEntity, Window, WindowBackgroundAppearance,
  WindowBounds, WindowDecorations, WindowId, WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;

use crate::{
  APP_NAME,
  bar::{BAR_NAMESPACE, base::Bar},
  config::{ConfigProvider, bar::BarConfig},
};

pub struct BarState {
  bars: HashMap<WindowId, WeakEntity<Bar>>,
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState {
      bars: HashMap::new(),
    });

    let config = cx.config();
    for bar in config.bars.clone() {
      if let Err(e) = Self::create(cx, bar) {
        error!("Failed to create bar: {}", e);
      }
    }
  }

  pub fn create(cx: &mut App, config: BarConfig) -> Result<()> {
    let flare = cx.theme().radius_2xl().as_f32();

    cx.open_window(
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
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Bar::new(config, cx));
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

    Ok(())
  }

  pub fn get(window: &Window, cx: &App) -> Option<Entity<Bar>> {
    cx.global::<BarState>()
      .bars
      .get(&window.window_handle().window_id())?
      .upgrade()
  }
}
