use std::collections::HashMap;

use anyhow::{Context as _, Result};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, placement::Placement};
use corona_utils::display::display_id_for;
use gpui_kit::{
  App, AppContext, Bounds, Context, DisplayId, Entity, EntityId, Global, Pixels, Size, Styled,
  WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  bar::{BarState, Widget, base::Bar},
  panel::{
    PANEL_NAME,
    align::Align,
    base::BasePanel,
    variants::{Panel, PanelData},
  },
};

struct OpenPanel {
  display: Option<DisplayId>,
  opener: Option<EntityId>,
  view: WeakEntity<BasePanel>,
}

pub struct PanelState {
  panels: HashMap<String, OpenPanel>,
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
    self.register_data(PanelData::new::<P>())
  }

  /// Registers a panel built at runtime, like a plugin's
  pub fn register_data(&mut self, data: PanelData) -> &mut Self {
    self.registry.insert(data.name.clone(), data);
    self
  }

  /// Forgets panel `name` and closes it when open
  pub fn unregister(name: &str, cx: &mut App) {
    cx.global_mut::<PanelState>().registry.remove(name);
    Self::close(name, cx).ok();
  }

  pub fn names(cx: &App) -> Vec<String> {
    let mut names: Vec<_> = cx.global::<PanelState>().registry.keys().cloned().collect();
    names.sort_unstable();
    names
  }

  pub(crate) fn toggle(
    data: PanelData,
    button_bounds: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    placement: Placement,
    opener: EntityId,
    window: &Window,
    cx: &mut App,
  ) -> Result<()> {
    let align = Align::from_bounds(button_bounds, bar_bounds, data.width, placement, cx);
    let display_id = window.display(cx).map(|d| d.id());
    Self::apply(data, align, placement, display_id, Some(opener), true, cx)
  }

  pub fn show(name: &str, toggle: bool, cx: &mut App) -> Result<()> {
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
    Self::apply(data, align, placement, Some(display_id), None, toggle, cx)
  }

  fn target_bar(cx: &App) -> Option<(DisplayId, Entity<Bar>)> {
    let compositor = cx.compositor();
    let active = compositor.active_monitor(cx);
    let monitors = compositor.list_monitors(cx).iter().filter(|m| !m.disabled);

    std::iter::once(active)
      .chain(monitors)
      .filter_map(|m| display_id_for(&m.name, cx))
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
    opener: Option<EntityId>,
    toggle: bool,
    cx: &mut App,
  ) -> Result<()> {
    let others: Vec<_> = cx
      .global::<PanelState>()
      .panels
      .iter()
      .filter(|(name, _)| **name != data.name)
      .filter_map(|(_, open)| open.view.upgrade())
      .collect();
    for panel in others {
      panel.update(cx, |panel, cx| panel.close(cx));
    }

    if let Some((display, panel)) = Self::get(&data.name, cx) {
      let (current, open) = panel.read_with(cx, |p, _| (p.align(), p.is_open()));
      let same_place = current == align && display == display_id;
      let same_opener =
        opener.is_none() || cx.global::<PanelState>().panels[&data.name].opener == opener;

      match decide(same_place, open, toggle, same_opener) {
        Action::Keep => {}
        Action::Close => panel.update(cx, |panel, cx| panel.close(cx)),
        Action::Reopen => panel.update(cx, |panel, cx| panel.open(cx)),
        Action::Replace => {
          panel.update(cx, |panel, cx| panel.close(cx));
          return Self::open_new(data, align, placement, display_id, opener, cx);
        }
      }
      Self::set_opener(&data.name, opener, cx);
      return Ok(());
    }

    Self::open_new(data, align, placement, display_id, opener, cx)
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
    opener: Option<EntityId>,
    cx: &mut App,
  ) -> Result<()> {
    cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: None,
          layer: Layer::Top,
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
        let view = cx.new(|cx| BasePanel::new(&data, align, placement, display_id, window, cx));
        let state = cx.global_mut::<PanelState>();
        state.panels.insert(
          data.name,
          OpenPanel {
            display: display_id,
            opener,
            view: view.downgrade(),
          },
        );

        cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
      },
    )?;

    Ok(())
  }

  fn get(name: &str, cx: &App) -> Option<(Option<DisplayId>, Entity<BasePanel>)> {
    let open = cx.global::<PanelState>().panels.get(name)?;
    Some((open.display, open.view.upgrade()?))
  }

  fn set_opener(name: &str, opener: Option<EntityId>, cx: &mut App) {
    if let Some(open) = cx.global_mut::<PanelState>().panels.get_mut(name) {
      open.opener = opener;
    }
  }
}

/// What showing a panel does to its open window
#[derive(Debug, PartialEq, Clone, Copy)]
enum Action {
  Keep,
  Close,
  Reopen,
  /// Close it and open a new one in the new place
  Replace,
}

fn decide(same_place: bool, open: bool, toggle: bool, same_opener: bool) -> Action {
  match (same_place, open) {
    (true, true) if toggle && same_opener => Action::Close,
    (true, true) => Action::Keep,
    (true, false) => Action::Reopen,
    (false, _) => Action::Replace,
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
      widget_id,
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

#[cfg(test)]
mod tests {
  use gpui_kit::TestAppContext;

  use super::*;
  use crate::{
    bar::BarExt,
    test_support::{self, PanelA, PanelB, draw_all, windows},
  };

  #[test]
  fn decide_covers_every_case() {
    for same_place in [true, false] {
      for open in [true, false] {
        for toggle in [true, false] {
          for same_opener in [true, false] {
            let expected = match (same_place, open) {
              (false, _) => Action::Replace,
              (true, false) => Action::Reopen,
              (true, true) if toggle && same_opener => Action::Close,
              (true, true) => Action::Keep,
            };
            assert_eq!(
              decide(same_place, open, toggle, same_opener),
              expected,
              "{same_place} {open} {toggle} {same_opener}"
            );
          }
        }
      }
    }
  }

  fn setup(cx: &mut TestAppContext) {
    test_support::setup(cx);
    test_support::no_animations(cx);
    cx.update(|cx| {
      cx.panel().register::<PanelA>().register::<PanelB>();
    });
  }

  fn apply<P: Panel>(
    align: Align,
    opener: Option<EntityId>,
    toggle: bool,
    cx: &mut TestAppContext,
  ) {
    cx.update(|cx| {
      PanelState::apply(
        PanelData::new::<P>(),
        align,
        Placement::Top,
        None,
        opener,
        toggle,
        cx,
      )
      .unwrap()
    });
    cx.run_until_parked();
  }

  fn panel(name: &str, cx: &mut TestAppContext) -> Option<Entity<BasePanel>> {
    cx.update(|cx| PanelState::get(name, cx).map(|(_, p)| p))
  }

  fn is_open(name: &str, cx: &mut TestAppContext) -> bool {
    panel(name, cx).is_some_and(|p| cx.update(|cx| p.read(cx).is_open()))
  }

  fn opener(name: &str, cx: &mut TestAppContext) -> Option<EntityId> {
    cx.update(|cx| cx.global::<PanelState>().panels[name].opener)
  }

  fn entity_id(cx: &mut TestAppContext) -> EntityId {
    cx.update(|cx| cx.new(|_| ()).entity_id())
  }

  #[gpui_kit::test]
  fn names_are_sorted(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| assert_eq!(PanelState::names(cx), ["panel_a", "panel_b"]));
  }

  #[gpui_kit::test]
  fn toggle_from_the_same_opener_closes(cx: &mut TestAppContext) {
    setup(cx);
    let button = entity_id(cx);
    let before = windows(cx);
    apply::<PanelA>(Align::Left, Some(button), true, cx);
    assert!(is_open("panel_a", cx));
    assert!(windows(cx) > before);
    assert_eq!(opener("panel_a", cx), Some(button));

    apply::<PanelA>(Align::Left, Some(button), true, cx);
    assert!(!is_open("panel_a", cx));
    // closed panels go away after their last frame
    draw_all(cx);
    draw_all(cx);
    assert!(panel("panel_a", cx).is_none());
    assert_eq!(windows(cx), before);
  }

  #[gpui_kit::test]
  fn another_opener_takes_the_panel_over(cx: &mut TestAppContext) {
    setup(cx);
    let (a, b) = (entity_id(cx), entity_id(cx));
    apply::<PanelA>(Align::Left, Some(a), true, cx);
    let first = panel("panel_a", cx).unwrap();
    apply::<PanelA>(Align::Left, Some(b), true, cx);
    assert!(is_open("panel_a", cx));
    assert_eq!(opener("panel_a", cx), Some(b));
    assert_eq!(panel("panel_a", cx).unwrap(), first);
  }

  #[gpui_kit::test]
  fn show_keeps_an_open_panel_and_toggle_without_opener_closes(cx: &mut TestAppContext) {
    setup(cx);
    apply::<PanelA>(Align::Left, None, false, cx);
    apply::<PanelA>(Align::Left, None, false, cx);
    assert!(is_open("panel_a", cx));
    apply::<PanelA>(Align::Left, None, true, cx);
    assert!(!is_open("panel_a", cx));
  }

  #[gpui_kit::test]
  fn a_closing_panel_reopens_in_place(cx: &mut TestAppContext) {
    setup(cx);
    apply::<PanelA>(Align::Right, None, false, cx);
    let first = panel("panel_a", cx).unwrap();
    cx.update(|cx| PanelState::close("panel_a", cx).unwrap());
    assert!(!is_open("panel_a", cx));
    apply::<PanelA>(Align::Right, None, false, cx);
    assert!(is_open("panel_a", cx));
    assert_eq!(panel("panel_a", cx).unwrap(), first);
  }

  #[gpui_kit::test]
  fn a_new_place_replaces_the_window(cx: &mut TestAppContext) {
    setup(cx);
    apply::<PanelA>(Align::Left, None, false, cx);
    let first = panel("panel_a", cx).unwrap();
    apply::<PanelA>(Align::Relative(300.), None, false, cx);
    let second = panel("panel_a", cx).unwrap();
    assert_ne!(first, second);
    cx.update(|cx| {
      assert!(!first.read(cx).is_open());
      assert!(second.read(cx).is_open());
      assert!(second.read(cx).align() == Align::Relative(300.));
    });
  }

  #[gpui_kit::test]
  fn opening_one_panel_closes_the_others(cx: &mut TestAppContext) {
    setup(cx);
    apply::<PanelA>(Align::Left, None, false, cx);
    apply::<PanelB>(Align::Left, None, false, cx);
    assert!(!is_open("panel_a", cx));
    assert!(is_open("panel_b", cx));
    cx.update(|cx| cx.close_panel::<PanelB>().unwrap());
    assert!(!is_open("panel_b", cx));
  }

  #[gpui_kit::test]
  fn panels_render_on_every_side(cx: &mut TestAppContext) {
    setup(cx);
    for (placement, align) in [
      (Placement::Top, Align::Left),
      (Placement::Bottom, Align::Right),
      (Placement::Left, Align::Relative(300.)),
      (Placement::Right, Align::Right),
    ] {
      cx.update(|cx| {
        PanelState::apply(
          PanelData::new::<PanelA>(),
          align,
          placement,
          None,
          None,
          false,
          cx,
        )
        .unwrap()
      });
      draw_all(cx);
      cx.update(|cx| PanelState::close("panel_a", cx).unwrap());
      draw_all(cx);
      draw_all(cx);
    }
  }

  #[gpui_kit::test]
  fn runtime_panels_register_and_unregister(cx: &mut TestAppContext) {
    setup(cx);
    let data = PanelData::from_fn("plugin:p", 123., 45., |window, cx| {
      cx.new(|cx| PanelA::init(window, cx)).into()
    });
    assert_eq!((data.width, data.height), (123., 45.));
    cx.update(|cx| {
      cx.panel().register_data(data.clone());
      assert!(PanelState::names(cx).contains(&"plugin:p".to_string()));
      PanelState::apply(data, Align::Left, Placement::Top, None, None, false, cx).unwrap();
    });
    assert!(is_open("plugin:p", cx));
    cx.update(|cx| PanelState::unregister("plugin:p", cx));
    assert!(!is_open("plugin:p", cx));
    cx.update(|cx| {
      assert!(!PanelState::names(cx).contains(&"plugin:p".to_string()));
      let err = PanelState::show("plugin:p", true, cx).unwrap_err();
      assert!(err.to_string().contains("unknown panel"));
    });
  }

  #[gpui_kit::test]
  fn show_needs_a_known_panel_and_a_bar(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let err = PanelState::show("nope", true, cx).unwrap_err();
      assert!(err.to_string().contains("unknown panel"));
      // the fake compositor's monitor has no display, so no bar either
      let err = PanelState::show("panel_a", true, cx).unwrap_err();
      assert!(err.to_string().contains("no bar"));
      assert!(PanelState::close("nope", cx).is_ok());
    });
  }

  #[gpui_kit::test]
  fn a_bar_button_toggles_its_panel(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      cx.bar_mut().register::<test_support::Toggle>();
    });
    let config = corona_config::bar::BarConfig {
      start: vec![],
      center: vec![corona_config::bar::WidgetConfig::Widget(
        corona_config::bar::WidgetEntry {
          widget_type: "toggle".into(),
          options: None,
        },
      )],
      end: vec![],
      ..Default::default()
    };
    let handle = cx.update(|cx| {
      let display = cx.displays()[0].id();
      BarState::create(cx, config, display).unwrap()
    });
    cx.simulate_window_resize(handle, gpui_kit::size(px(1000.), px(40.)));
    draw_all(cx);
    let toggle = |cx: &mut TestAppContext| {
      cx.update_window(handle, |_, window, cx| {
        let bar = BarState::get(window, cx).unwrap();
        let view = bar.read(cx).sections()[1][0][0].clone();
        let widget = view.downcast::<test_support::Toggle>().unwrap();
        widget.update(cx, |_, cx| cx.toggle_panel::<PanelA>(window))
      })
      .unwrap()
      .unwrap();
      cx.run_until_parked();
    };
    toggle(cx);
    assert!(is_open("panel_a", cx));
    // a centered button keeps the panel under it
    let align = panel("panel_a", cx).map(|p| cx.update(|cx| p.read(cx).align()));
    assert!(matches!(align, Some(Align::Relative(_))));
    toggle(cx);
    assert!(!is_open("panel_a", cx));
  }

  #[gpui_kit::test]
  fn toggle_panel_needs_a_bar_window(cx: &mut TestAppContext) {
    setup(cx);
    let handle = test_support::plain_window(cx);
    let result = cx
      .update_window(handle, |_, window, cx| {
        let widget = cx.new(|_| test_support::Toggle { danger: false });
        widget.update(cx, |_, cx| cx.toggle_panel::<PanelA>(window))
      })
      .unwrap();
    assert!(result.unwrap_err().to_string().contains("no bar"));
  }
}
