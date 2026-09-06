use std::collections::HashMap;

use anyhow::Result;
use gpui_kit::{
  App, AppContext, Bounds, Entity, Global, Pixels, Size, Styled, WeakEntity,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::panel::base::BasePanel;

mod anim;
mod base;

pub struct PanelState {
  panels: HashMap<String, WeakEntity<BasePanel>>,
}

impl Global for PanelState {}

impl PanelState {
  pub fn init(cx: &mut App) {
    cx.set_global(PanelState {
      panels: HashMap::new(),
    });
  }

  fn get(name: &str, cx: &App) -> Option<Entity<BasePanel>> {
    cx.global::<PanelState>().panels.get(name)?.upgrade()
  }

  pub fn open(
    name: String,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    cx: &mut App,
  ) -> Result<()> {
    if let Some(panel) = Self::get(&name, cx) {
      panel.update(cx, |panel, cx| panel.open(cx));
      return Ok(());
    }

    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          // Covers the whole output — including under the bar, which is what
          // an exclusive zone of -1 asks for — so a click anywhere off the
          // panel reaches this window and dismisses it.
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: "corona_panel".to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        app_id: Some("corona_bar".to_string()),
        titlebar: None,
        // A zero size with opposite anchors means "fill"; a real size would be
        // centered between them instead. The bar does the same for its width.
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view =
          cx.new(|cx| BasePanel::new(window.window_handle(), button_bounds, bar_bounds, cx));
        let state = cx.global_mut::<PanelState>();
        state.panels.insert(name, view.downgrade());

        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )?;

    Ok(())
  }

  pub fn close(name: &str, cx: &mut App) {
    if let Some(panel) = Self::get(name, cx) {
      panel.update(cx, |panel, cx| panel.close(cx));
    }
  }

  pub fn is_open(name: &str, cx: &mut App) -> bool {
    Self::get(name, cx).is_some_and(|panel| panel.read(cx).is_open())
  }
}
