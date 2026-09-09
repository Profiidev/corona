use gpui_kit::App;

mod animation;
mod app;
mod assets;
mod bar;
mod components;
mod panel;
mod tooltip;
mod utils;

pub use app::WidgetType;
pub use assets::Assets;

pub fn init(cx: &mut App) {
  assets::load(cx).expect("Failed to load themes");
  bar::BarState::init(cx);
  panel::PanelState::init(cx);
  tooltip::TooltipState::init(cx);
}
