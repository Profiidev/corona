use std::collections::HashMap;

use anyhow::Result;
use gpui_kit::{
  App, AppContext, Bounds, Entity, Global, Pixels, Size, Styled, WeakEntity,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  APP_NAME,
  panel::{PANEL_NAME, align::Align, base::BasePanel, variants::Panel},
};

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

  pub fn toggle<P: Panel>(
    panel: P,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    cx: &mut App,
  ) -> Result<()> {
    let new_align = Align::from_bounds(button_bounds, bar_bounds, P::WIDTH, cx);

    if let Some(panel) = Self::get(P::NAME, cx) {
      let (align, open) = panel.read_with(cx, |p, _| (p.align(), p.is_open()));

      match (align == new_align, open) {
        (true, true) => {
          panel.update(cx, |panel, cx| panel.close(cx));
          return Ok(());
        }
        (true, false) => {
          panel.update(cx, |panel, cx| panel.open(cx));
          return Ok(());
        }
        (false, _) => {
          panel.update(cx, |panel, cx| panel.close(cx));
        }
      }
    }

    Self::open_new::<P>(panel, new_align, cx)
  }

  fn open_new<P: Panel>(panel: P, align: Align, cx: &mut App) -> Result<()> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: PANEL_NAME.to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| BasePanel::new(window.window_handle(), panel, align, cx));
        let state = cx.global_mut::<PanelState>();
        state.panels.insert(P::NAME.to_string(), view.downgrade());

        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )?;

    Ok(())
  }

  fn get(name: &str, cx: &App) -> Option<Entity<BasePanel>> {
    cx.global::<PanelState>().panels.get(name)?.upgrade()
  }
}
