use anyhow::Result;
use gpui_kit::App;

pub mod bar;
pub mod commands;
pub mod input_region;
pub mod osd;
pub mod panel;
pub mod per_display;
pub mod popup;
pub mod tooltip;

pub fn init(cx: &mut App) -> Result<()> {
  bar::BarState::init(cx);
  panel::PanelState::init(cx);
  tooltip::TooltipState::init(cx);
  osd::OsdState::init(cx);
  Ok(())
}
