use std::collections::HashMap;

use anyhow::{Context as _, Result};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, placement::Placement};
use gpui_kit::{
  App, AppContext, Bounds, Context, DisplayId, Entity, Global, Pixels, Size, Styled, WeakEntity,
  Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  bar::{BarExt, BarState, Widget, base::Bar},
  panel::{
    PANEL_NAME,
    align::Align,
    base::BasePanel,
    variants::{Panel, PanelData},
  },
};

pub struct PanelState {
  panels: HashMap<String, (Option<DisplayId>, WeakEntity<BasePanel>)>,
  registry: HashMap<String, PanelData>,
}

impl Global for PanelState {}

impl PanelState {
  pub fn init(cx: &mut App) {
    cx.set_global(PanelState {
      panels: HashMap::new(),
      registry: HashMap::new(),
    });
  }

  pub fn register<P: Panel>(&mut self) -> &mut Self {
    let data = PanelData::new::<P>();
    self.registry.insert(data.name.clone(), data);
    self
  }

  pub(crate) fn toggle(
    data: PanelData,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    placement: Placement,
    window: &Window,
    cx: &mut App,
  ) -> Result<()> {
    let align = Align::from_bounds(button_bounds, bar_bounds, data.width, placement, cx);
    let display_id = window.display(cx).map(|d| d.id());
    Self::apply(data, align, placement, display_id, true, cx)
  }

  pub(crate) fn show(name: &str, toggle: bool, cx: &mut App) -> Result<()> {
    let data = cx
      .global::<PanelState>()
      .registry
      .get(name)
      .cloned()
      .with_context(|| format!("unknown panel: {name}"))?;
    let (display_id, bar) = Self::target_bar(cx).context("no bar to attach the panel to")?;

    let bar = bar.read(cx);
    let placement = bar.placement();

    let align = Align::from_bounds(bar.bounds(), bar.bounds(), data.width, placement, cx);
    Self::apply(data, align, placement, Some(display_id), toggle, cx)
  }

  fn target_bar(cx: &App) -> Option<(DisplayId, Entity<Bar>)> {
    let compositor = cx.compositor();
    let active = compositor.active_monitor(cx);
    let monitors = compositor.list_monitors(cx).iter().filter(|m| !m.disabled);

    std::iter::once(active)
      .chain(monitors)
      .filter_map(|m| cx.bar().display_id_for(&m.name))
      .find_map(|display_id| {
        let bars = BarState::bars_on(display_id, cx);
        Some((display_id, bars.first()?.clone()))
      })
  }

  fn apply(
    data: PanelData,
    align: Align,
    placement: Placement,
    display_id: Option<DisplayId>,
    toggle: bool,
    cx: &mut App,
  ) -> Result<()> {
    let others: Vec<_> = cx
      .global::<PanelState>()
      .panels
      .iter()
      .filter(|(name, _)| **name != data.name)
      .filter_map(|(_, (_, panel))| panel.upgrade())
      .collect();
    for panel in others {
      panel.update(cx, |panel, cx| panel.close(cx));
    }

    if let Some((display, panel)) = Self::get(&data.name, cx) {
      let (current, open) = panel.read_with(cx, |p, _| (p.align(), p.is_open()));

      match (current == align && display == display_id, open) {
        (true, true) => {
          if toggle {
            panel.update(cx, |panel, cx| panel.close(cx));
          }
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

    Self::open_new(data, align, placement, display_id, cx)
  }

  pub(crate) fn close(name: &str, cx: &mut App) -> Result<()> {
    if let Some((_, panel)) = Self::get(name, cx) {
      panel.update(cx, |panel, cx| panel.close(cx));
    }

    Ok(())
  }

  fn open_new(
    data: PanelData,
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
        let view = cx.new(|cx| BasePanel::new(&data, align, placement, window, cx));
        let state = cx.global_mut::<PanelState>();
        state
          .panels
          .insert(data.name, (display_id, view.downgrade()));

        cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
      },
    )?;

    Ok(())
  }

  fn get(name: &str, cx: &App) -> Option<(Option<DisplayId>, Entity<BasePanel>)> {
    let (display, panel) = cx.global::<PanelState>().panels.get(name)?;
    Some((*display, panel.upgrade()?))
  }
}

pub trait WdigetPanelExt {
  fn toggle_panel<P: Panel>(&mut self, window: &Window) -> Result<()>;
}

impl<W: Widget> WdigetPanelExt for Context<'_, W> {
  fn toggle_panel<P: Panel>(&mut self, window: &Window) -> Result<()> {
    let widget_id = self.entity_id();
    let bar = BarState::get(window, self)
      .context("no bar in this window")?
      .read(self);
    let button_bounds = bar
      .widget_bounds(widget_id)
      .context("no bounds for this widget")?;

    PanelState::toggle(
      PanelData::new::<P>(),
      button_bounds,
      bar.bounds(),
      bar.placement(),
      window,
      self,
    )
  }
}

pub trait AppPanelExt {
  fn panel(&mut self) -> &mut PanelState;
  fn close_panel<P: Panel>(&mut self) -> Result<()>;
}

impl AppPanelExt for App {
  fn panel(&mut self) -> &mut PanelState {
    self.global_mut::<PanelState>()
  }

  fn close_panel<P: Panel>(&mut self) -> Result<()> {
    PanelState::close(P::NAME, self)
  }
}
