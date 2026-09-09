use std::collections::HashMap;

use anyhow::{Context as _, Result};
use gpui_kit::{
  App, AppContext, Bounds, Context, DisplayId, Entity, Global, Pixels, Size, Styled, WeakEntity,
  Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  APP_NAME,
  config::placement::Placement,
  ui::{
    bar::{BarState, Widget},
    panel::{PANEL_NAME, align::Align, base::BasePanel, variants::Panel},
  },
};

pub struct PanelState {
  panels: HashMap<String, (Option<DisplayId>, WeakEntity<BasePanel>)>,
}

impl Global for PanelState {}

impl PanelState {
  pub fn init(cx: &mut App) {
    cx.set_global(PanelState {
      panels: HashMap::new(),
    });
  }

  pub fn toggle<P: Panel>(
    panel: impl FnOnce() -> P,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    placement: Placement,
    window: &Window,
    cx: &mut App,
  ) -> Result<()> {
    let new_align = Align::from_bounds(button_bounds, bar_bounds, P::WIDTH, placement, cx);
    let display_id = window.display(cx).map(|d| d.id());

    if let Some((display, panel)) = Self::get(P::NAME, cx) {
      let (align, open) = panel.read_with(cx, |p, _| (p.align(), p.is_open()));

      match (align == new_align && display == display_id, open) {
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

    Self::open_new::<P>(panel(), new_align, placement, display_id, cx)
  }

  fn open_new<P: Panel>(
    panel: P,
    align: Align,
    placement: Placement,
    display_id: Option<DisplayId>,
    cx: &mut App,
  ) -> Result<()> {
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
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        display_id,
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| BasePanel::new(panel, align, placement, cx));
        let state = cx.global_mut::<PanelState>();
        state
          .panels
          .insert(P::NAME.to_string(), (display_id, view.downgrade()));

        cx.new(|cx| {
          Root::new(view, window, cx)
            .bordered(false)
            .bg(gpui_kit::transparent_black())
        })
      },
    )?;

    Ok(())
  }

  fn get(name: &str, cx: &App) -> Option<(Option<DisplayId>, Entity<BasePanel>)> {
    let (display, panel) = cx.global::<PanelState>().panels.get(name)?;
    Some((*display, panel.upgrade()?))
  }
}

pub trait PanelExt {
  fn toggle_panel<P: Panel>(&mut self, panel: impl FnOnce() -> P, window: &Window) -> Result<()>;
}

impl<W: Widget> PanelExt for Context<'_, W> {
  fn toggle_panel<P: Panel>(&mut self, panel: impl FnOnce() -> P, window: &Window) -> Result<()> {
    let widget_id = self.entity_id();
    let bar = BarState::get(window, self)
      .context("no bar in this window")?
      .read(self);
    let button_bounds = bar
      .widget_bounds(widget_id)
      .context("no bounds for this widget")?;

    PanelState::toggle(
      panel,
      button_bounds,
      bar.bounds(),
      bar.placement(),
      window,
      self,
    )
  }
}
