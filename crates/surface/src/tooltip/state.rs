use std::collections::HashMap;

use anyhow::{Context, Result};
use corona_config::placement::Placement;
use gpui_kit::{
  AnyView, AnyWindowHandle, App, AppContext, Bounds, Global, Pixels, Size, Styled, WeakEntity,
  Window, component::Root, px,
};

use crate::{
  bar::BarState,
  popup::popup_options,
  tooltip::{
    base::{BORDER, BaseTooltip},
    variants::Tooltip,
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
    cx.open_window(
      popup_options(parent, anchor, placement, size, px(TOOLTIP_GAP), false),
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

        cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
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

#[cfg(test)]
mod tests {
  use gpui_kit::{TestAppContext, point, size};

  use super::*;
  use crate::test_support::{self, TipA, TipB, draw_all, plain_window, windows};

  fn anchor(x: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(0.)), size(px(10.), px(10.)))
  }

  fn show<T: Tooltip>(
    tip: T,
    parent: AnyWindowHandle,
    at: Bounds<Pixels>,
    cx: &mut TestAppContext,
  ) -> Result<()> {
    cx.update_window(parent, |_, window, cx| {
      cx.show_tooltip(tip, at, Placement::Top, window)
    })
    .unwrap()
  }

  fn entry(name: &str, cx: &mut TestAppContext) -> Option<(AnyWindowHandle, Size<Pixels>)> {
    cx.update(|cx| {
      cx.global::<TooltipState>()
        .tooltips
        .get(name)
        .map(|e| (e.handle, e.size))
    })
  }

  #[gpui_kit::test]
  fn same_place_reuses_and_resizes(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let parent = plain_window(cx);
    let before = windows(cx);
    show(TipA::<40>, parent, anchor(0.), cx).unwrap();
    let (handle, first) = entry("tip_a", cx).unwrap();
    // the border goes around the content
    assert_eq!(first, size(px(40. + BORDER * 2.), px(20. + BORDER * 2.)));
    assert_eq!(windows(cx), before + 1);
    draw_all(cx);

    show(TipA::<40>, parent, anchor(0.), cx).unwrap();
    show(TipA::<80>, parent, anchor(0.), cx).unwrap();
    let (again, resized) = entry("tip_a", cx).unwrap();
    assert!(again == handle);
    assert_eq!(resized.width, px(80. + BORDER * 2.));
    assert_eq!(windows(cx), before + 1);
  }

  #[gpui_kit::test]
  fn a_new_anchor_replaces_the_window(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let parent = plain_window(cx);
    let before = windows(cx);
    show(TipA::<40>, parent, anchor(0.), cx).unwrap();
    let (first, _) = entry("tip_a", cx).unwrap();
    show(TipA::<40>, parent, anchor(50.), cx).unwrap();
    let (second, _) = entry("tip_a", cx).unwrap();
    assert!(first != second);
    cx.executor()
      .advance_clock(std::time::Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(windows(cx), before + 1);
  }

  #[gpui_kit::test]
  fn hide_forgets_at_once_and_closes_after_a_moment(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let parent = plain_window(cx);
    let before = windows(cx);
    show(TipA::<40>, parent, anchor(0.), cx).unwrap();
    show(TipB, parent, anchor(0.), cx).unwrap();
    cx.update(|cx| cx.hide_tooltip::<TipA<40>>());
    assert!(entry("tip_a", cx).is_none());
    assert!(entry("tip_b", cx).is_some());
    assert_eq!(windows(cx), before + 2);
    cx.executor()
      .advance_clock(std::time::Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(windows(cx), before + 1);
    // nothing to hide
    cx.update(|cx| cx.hide_tooltip::<TipA<40>>());
  }

  #[gpui_kit::test]
  fn bar_tooltips_need_a_bar(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let parent = plain_window(cx);
    let result = cx
      .update_window(parent, |_, window, cx| {
        cx.show_bar_tooltip(TipB, anchor(0.), window)
      })
      .unwrap();
    assert!(result.unwrap_err().to_string().contains("no bar"));
  }
}
