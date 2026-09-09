use std::collections::HashMap;

use anyhow::Result;
use gpui_kit::{
  AnyView, AnyWindowHandle, App, AppContext, Bounds, DisplayId, Global, Pixels, Size, Styled,
  WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  component::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use crate::{
  APP_NAME,
  config::placement::Placement,
  ui::tooltip::{TOOLTIP_NAME, align::Align, base::BaseTooltip, variants::Tooltip},
};

struct Entry {
  display: Option<DisplayId>,
  handle: AnyWindowHandle,
  view: WeakEntity<BaseTooltip>,
}

pub struct TooltipState {
  tooltips: HashMap<&'static str, Entry>,
}

impl Global for TooltipState {}

impl TooltipState {
  pub fn init(cx: &mut App) {
    cx.set_global(TooltipState {
      tooltips: HashMap::new(),
    });
  }

  pub fn show<T: Tooltip>(
    tooltip: T,
    anchor: Bounds<Pixels>,
    bar_bounds: Bounds<Pixels>,
    placement: Placement,
    window: &Window,
    cx: &mut App,
  ) -> Result<()> {
    let size = tooltip.size(window, cx);
    let align = Align::from_bounds(anchor, bar_bounds, size, placement);
    let display_id = window.display(cx).map(|d| d.id());
    let tooltip = cx.new(|_| tooltip).into();

    if let Some(entry) = cx.global::<TooltipState>().tooltips.get(T::NAME)
      && let Some(view) = entry.view.upgrade()
    {
      if entry.display == display_id {
        view.update(cx, |this, cx| this.show(tooltip, align, size, cx));
        return Ok(());
      }

      Self::hide::<T>(cx);
    }

    Self::open_new::<T>(tooltip, align, size, placement, display_id, cx)
  }

  pub fn hide<T: Tooltip>(cx: &mut App) {
    let Some(entry) = cx.global_mut::<TooltipState>().tooltips.remove(T::NAME) else {
      return;
    };

    let _ = entry
      .handle
      .update(cx, |_, window, _| window.remove_window());
  }

  fn open_new<T: Tooltip>(
    tooltip: AnyView,
    align: Align,
    size: Size<Pixels>,
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
          namespace: TOOLTIP_NAME.to_string(),
          keyboard_interactivity: KeyboardInteractivity::None,
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
        let view = cx.new(|_| BaseTooltip::new(tooltip, align, size, placement));

        let state = cx.global_mut::<TooltipState>();
        state.tooltips.insert(
          T::NAME,
          Entry {
            display: display_id,
            handle: window.window_handle(),
            view: view.downgrade(),
          },
        );

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
