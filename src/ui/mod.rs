use gpui_kit::App;

mod animation;
mod assets;
mod bar;
mod components;
mod panel;

pub use assets::Assets;
pub use bar::WidgetType;

pub fn init(cx: &mut App) {
  assets::load(cx).expect("Failed to load themes");
  bar::BarState::init(cx);
  panel::PanelState::init(cx);
}
