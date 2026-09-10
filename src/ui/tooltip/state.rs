use std::collections::HashMap;

use anyhow::{Context, Result};
use gpui_kit::{
  AnyView, AnyWindowHandle, App, AppContext, Bounds, Global, Pixels, Point, Size, Styled,
  WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  component::Root,
  point,
  popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions},
  px,
};

use crate::{
  APP_NAME,
  config::placement::Placement,
  ui::{
    bar::BarState,
    tooltip::{
      base::{BORDER, BaseTooltip},
      variants::Tooltip,
    },
  },
};

const TOOLTIP_GAP: f32 = 4.;

struct Entry {
  parent: AnyWindowHandle,
  anchor: Bounds<Pixels>,
  size: Size<Pixels>,
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
    placement: Placement,
    window: &Window,
    cx: &mut App,
  ) -> Result<()> {
    let parent = window.window_handle();
    let size = tooltip.size(window, cx);
    let size = Size::new(size.width + px(BORDER * 2.), size.height + px(BORDER * 2.));
    let tooltip = cx.new(|_| tooltip).into();

    if let Some(entry) = cx.global::<TooltipState>().tooltips.get(T::NAME)
      && let Some(view) = entry.view.upgrade()
    {
      if entry.parent == parent && entry.anchor == anchor {
        let resize = entry.size != size;
        let handle = entry.handle;

        view.update(cx, |this, cx| this.show(tooltip, cx));

        if resize {
          let _ = handle.update(cx, |_, window, _| window.resize(size));
          if let Some(entry) = cx.global_mut::<TooltipState>().tooltips.get_mut(T::NAME) {
            entry.size = size
          }
        }

        return Ok(());
      }

      Self::hide::<T>(cx);
    }

    Self::open_new::<T>(tooltip, parent, anchor, size, placement, cx)
  }

  pub fn hide<T: Tooltip>(cx: &mut App) {
    let Some(entry) = cx.global_mut::<TooltipState>().tooltips.remove(T::NAME) else {
      return;
    };

    cx.spawn(async move |cx| {
      cx.background_executor()
        .timer(std::time::Duration::from_millis(1))
        .await;
      let _ = entry.handle.update(cx, |_, window, _| {
        window.remove_window();
      });
    })
    .detach();
  }

  fn open_new<T: Tooltip>(
    tooltip: AnyView,
    parent: AnyWindowHandle,
    anchor: Bounds<Pixels>,
    size: Size<Pixels>,
    placement: Placement,
    cx: &mut App,
  ) -> Result<()> {
    let gap = px(TOOLTIP_GAP);
    let (popup_anchor, gravity, offset) = match placement {
      Placement::Top => (
        PopupAnchor::Bottom,
        PopupGravity::Bottom,
        point(px(0.), gap),
      ),
      Placement::Bottom => (PopupAnchor::Top, PopupGravity::Top, point(px(0.), -gap)),
      Placement::Left => (PopupAnchor::Right, PopupGravity::Right, point(gap, px(0.))),
      Placement::Right => (PopupAnchor::Left, PopupGravity::Left, point(-gap, px(0.))),
    };

    cx.open_window(
      WindowOptions {
        kind: WindowKind::AnchoredPopup(PopupOptions {
          parent,
          anchor_rect: anchor,
          anchor: popup_anchor,
          gravity,
          constraint_adjustment: PopupConstraintAdjustment::SLIDE_X
            | PopupConstraintAdjustment::SLIDE_Y,
          offset,
          grab: false,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: Point::default(),
          size,
        })),
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|_| BaseTooltip::new(tooltip));

        let state = cx.global_mut::<TooltipState>();
        state.tooltips.insert(
          T::NAME,
          Entry {
            parent,
            anchor,
            size,
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

pub trait TooltipExt {
  fn show_tooltip<T: Tooltip>(
    &mut self,
    tooltip: T,
    anchor: Bounds<Pixels>,
    placement: Placement,
    window: &Window,
  ) -> Result<()>;

  fn show_bar_tooltip<T: Tooltip>(
    &mut self,
    tooltip: T,
    anchor: Bounds<Pixels>,
    window: &Window,
  ) -> Result<()>;

  fn hide_tooltip<T: Tooltip>(&mut self);
}

impl TooltipExt for App {
  fn show_tooltip<T: Tooltip>(
    &mut self,
    tooltip: T,
    anchor: Bounds<Pixels>,
    placement: Placement,
    window: &Window,
  ) -> Result<()> {
    TooltipState::show(tooltip, anchor, placement, window, self)
  }

  fn show_bar_tooltip<T: Tooltip>(
    &mut self,
    tooltip: T,
    anchor: Bounds<Pixels>,
    window: &Window,
  ) -> Result<()> {
    let bar = BarState::get(window, self)
      .context("no bar in this window")?
      .read(self);

    self.show_tooltip(tooltip, anchor, bar.placement(), window)
  }

  fn hide_tooltip<T: Tooltip>(&mut self) {
    TooltipState::hide::<T>(self);
  }
}
