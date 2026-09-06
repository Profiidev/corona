use anyhow::{Context, Result};
use gpui_kit::{App, Bounds, Pixels, Window};

use crate::panel::{Panel, PanelState};

pub mod base;
mod state;
mod widgets;

pub use state::BarState;
pub use widgets::WidgetType;

const BAR_NAMESPACE: &str = "corona_bar";

pub fn toggle_panel<P: Panel>(
  panel: P,
  widget: Bounds<Pixels>,
  window: &Window,
  cx: &mut App,
) -> Result<()> {
  let bar = BarState::get(window, cx).context("no bar in this window")?;
  let (bounds, placement) = bar.read(cx).geometry();

  PanelState::toggle(panel, widget, bounds, placement, cx)
}
