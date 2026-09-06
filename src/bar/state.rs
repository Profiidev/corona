use anyhow::Result;
use gpui_kit::{
  App, AppContext, Bounds, Global, Styled, WeakEntity, WindowBackgroundAppearance, WindowBounds,
  WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  APP_NAME,
  bar::{BAR_NAMESPACE, Placement, base::Bar},
};

pub struct BarState {
  bars: Vec<WeakEntity<Bar>>,
}

impl Global for BarState {}

impl BarState {
  pub fn init(cx: &mut gpui_kit::App) {
    cx.set_global(BarState { bars: Vec::new() });
  }

  pub fn create(cx: &mut App, placement: Placement, height: f32) -> Result<()> {
    let flare = cx.theme().radius_2xl().as_f32();

    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: placement.anchor(),
          exclusive_zone: Some(px(height)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Top,
          namespace: BAR_NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: placement.size(height + flare, 0.),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| Bar::new(placement, height));
        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )?;

    Ok(())
  }
}
