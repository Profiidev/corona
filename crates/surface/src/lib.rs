use anyhow::Result;
use gpui_kit::App;

use crate::bar::WidgetFactory;

pub mod bar;
pub mod panel;
pub mod tooltip;

pub fn init(cx: &mut App, widget: WidgetFactory) -> Result<()> {
  bar::BarState::init(cx, widget);
  panel::PanelState::init(cx);
  tooltip::TooltipState::init(cx);
  Ok(())
}
