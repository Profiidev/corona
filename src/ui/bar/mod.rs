use anyhow::{Context as _, Result};
use gpui_kit::{App, Bounds, Context, Pixels, Window};

use crate::{
  ui::bar::widgets::Widget,
  ui::panel::{Panel, PanelState},
  ui::tooltip::{Tooltip, TooltipState},
};

pub mod base;
mod state;
mod style;
mod widgets;

pub use state::BarState;
pub use widgets::WidgetType;

const BAR_NAMESPACE: &str = "corona_bar";

pub fn toggle_panel<P: Panel, W: Widget>(
  panel: impl FnOnce() -> P,
  window: &Window,
  cx: &mut Context<'_, W>,
) -> Result<()> {
  let widget_id = cx.entity_id();
  let bar = BarState::get(window, cx)
    .context("no bar in this window")?
    .read(cx);
  let button_bounds = bar
    .widget_bounds(widget_id)
    .context("no bounds for this widget")?;

  PanelState::toggle(
    panel,
    button_bounds,
    bar.bounds(),
    bar.placement(),
    window,
    cx,
  )
}

pub fn show_tooltip<T: Tooltip>(
  tooltip: T,
  anchor: Bounds<Pixels>,
  window: &Window,
  cx: &mut App,
) -> Result<()> {
  let bar = BarState::get(window, cx)
    .context("no bar in this window")?
    .read(cx);

  TooltipState::show(tooltip, anchor, bar.placement(), window, cx)
}

pub fn hide_tooltip<T: Tooltip>(cx: &mut App) {
  TooltipState::hide::<T>(cx);
}
